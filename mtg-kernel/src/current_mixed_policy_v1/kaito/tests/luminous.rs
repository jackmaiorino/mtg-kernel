use super::*;
use crate::{
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_luminous_regular_and_real_animated_targets_tap_discount_payment_counter_and_restore_both_seats(
) {
    let card = crate::card_def::card_id_by_name("Luminous Rebuke").unwrap();
    let meaning = super::super::super::roots::baseline(card).unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(card).unwrap(),
        Some(meaning)
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for q in [p, p.opponent()] {
            for animated in [false, true] {
                for tapped_before_cast in [false, true] {
                    let mut state = GameState::new_from_libraries_with_starting_player_v1(
                        &[forest; 40],
                        &[forest; 40],
                        crate::rl::card_name,
                        110_230,
                        q,
                    );
                    state.step = Step::Main1;
                    for _ in 0..6 {
                        put(&mut state, p, "Plains", Zone::Battlefield);
                    }
                    for _ in 0..7 {
                        put(&mut state, q, "Forest", Zone::Battlefield);
                    }
                    for name in ["Island", "Swamp"] {
                        for _ in 0..3 {
                            put(&mut state, p.opponent(), name, Zone::Battlefield);
                        }
                    }
                    let target = put(
                        &mut state,
                        q,
                        if animated {
                            "Soulstone Sanctuary"
                        } else {
                            "Llanowar Elves"
                        },
                        Zone::Battlefield,
                    );
                    let spell = put(&mut state, p, "Luminous Rebuke", Zone::Hand);
                    let counter = put(&mut state, p.opponent(), "Counterspell", Zone::Hand);
                    let removal = put(&mut state, p.opponent(), "Hero's Downfall", Zone::Hand);
                    let mut native =
                        FastActorSessionV1::from_kaito_v75_fixture_state(state.clone());
                    let mut own = KaitoSession::from_session(
                        FastActorSessionV1::from_kaito_v75_fixture_state(state),
                    )
                    .unwrap();
                    if animated {
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==target.0),
                        );
                        settle(&mut own, &mut native);
                        // Two natural quiet turns untap the permanent and all
                        // resources after its actual paid permanent animation.
                        super::ordinary_equipment::reach_main(&mut own, &mut native, q.opponent());
                        super::ordinary_equipment::reach_main(&mut own, &mut native, q);
                        assert!(crate::engine::object_has_type(
                            own.fixture_state(),
                            target,
                            crate::card_def::CardType::Creature
                        ));
                        assert!(crate::engine::object_has_type(
                            own.fixture_state(),
                            target,
                            crate::card_def::CardType::Land
                        ));
                    }
                    assert!(!own.fixture_state().objects.get(target).tapped);
                    if tapped_before_cast {
                        act(
                            &mut own,
                            &mut native,
                            |a| matches!(a,A::ActivateManaAbility{source,..}if source.arena_id==target.0),
                        );
                        assert!(own.fixture_state().objects.get(target).tapped);
                    }
                    if p != q {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                    }
                    let before = own.fixture_state();
                    let tapped_before = before.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| before.objects.get(o).tapped)
                        .count();
                    let floating_before: usize = before.players[p.index()]
                        .mana_pool
                        .iter()
                        .map(|&n| usize::from(n))
                        .sum();
                    assert_eq!(floating_before, usize::from(tapped_before_cast && p == q));
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==spell.0),
                    );
                    act(
                        &mut own,
                        &mut native,
                        |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==target.0),
                    );
                    let paid = if tapped_before_cast { 2 } else { 5 };
                    let state = own.fixture_state();
                    assert_eq!(state.objects.get(spell).zone, Zone::Stack);
                    assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                    let payment_taps = state.players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| state.objects.get(o).tapped)
                        .count()
                        - tapped_before;
                    assert_eq!(
                        payment_taps + floating_before,
                        paid,
                        "only tap state at cost lock changes the printed price"
                    );
                    let target_tapped_after_payment = state.objects.get(target).tapped;
                    let saved = own.snapshot();
                    let raw = native.snapshot_v1();
                    for response in [0, 1, 2] {
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
                                        matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==target.0)
                                    }
                                });
                            }
                            settle(&mut own, &mut native);
                            let state = own.fixture_state();
                            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
                            assert_eq!(
                                state.objects.get(target).zone,
                                if response == 1 {
                                    Zone::Battlefield
                                } else {
                                    Zone::Graveyard
                                }
                            );
                            assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
                            let still_tapped = state.players[p.index()]
                                .battlefield
                                .iter()
                                .filter(|&&o| state.objects.get(o).tapped)
                                .count();
                            let removed_tapped_target =
                                usize::from(p == q && response != 1 && target_tapped_after_payment);
                            assert_eq!(
                                still_tapped + removed_tapped_target,
                                payment_taps + tapped_before
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
        }
    }
}
