use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    card_def::CardType,
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_self_reflection_normal_current_copiable_values_tokens_anonymous_animated_land_counter_illegal_restore_both_seats(
) {
    paid(false);
}
#[test]
fn paid_self_reflection_flashback_four_mana_stack_value_copy_and_exile_counter_illegal_restore_both_seats(
) {
    paid(true);
}

fn paid(flashback: bool) {
    let id = crate::card_def::card_id_by_name("Self-Reflection").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for scene in 0..6 {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_394,
                p,
            );
            s.step = Step::Main1;
            let lands = (0..10)
                .map(|_| put(&mut s, p, "Island", Zone::Battlefield))
                .collect::<Vec<_>>();
            for name in ["Island", "Island", "Swamp", "Swamp"] {
                put(&mut s, p.opponent(), name, Zone::Battlefield);
            }
            let owner = if scene == 0 { p.opponent() } else { p };
            let target = put(
                &mut s,
                owner,
                match scene {
                    0 => "Serra Angel",
                    1 => "Soldier Token",
                    2 => "Dazzling Angel",
                    3 => "Llanowar Elves",
                    4 => "Serra Angel",
                    _ => "Soulstone Sanctuary",
                },
                Zone::Battlefield,
            );
            // Initial support operands, including a stolen creature and two
            // distinct hidden originals. Their acquisition is not claimed.
            if owner != p {
                s.players[owner.index()]
                    .battlefield
                    .retain(|&o| o != target);
                s.players[p.index()].battlefield.push(target);
                s.objects.get_mut(target).controller = p;
            }
            if scene == 0 {
                let o = s.objects.get_mut(target);
                o.tapped = true;
                o.damage = 1;
                o.counters.plus1_plus1 = 2;
            }
            if scene == 3 || scene == 4 {
                s.objects.get_mut(target).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                    disguised: true,
                    lookers: 1 << p.index(),
                    hidden_by: None,
                    hidden_by_source: None,
                });
            }
            let enemy = put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield);
            let source = put(
                &mut s,
                p,
                "Self-Reflection",
                if flashback {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                },
            );
            let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
            let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
            let next_creature = if scene == 2 {
                Some(put(&mut s, p, "Spectral Sailor", Zone::Hand))
            } else {
                None
            };
            let (mut own, mut native) = sessions(s);
            if scene == 5 {
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a, A::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == target.0),
                );
                settle(&mut own, &mut native);
                assert!(crate::engine::object_has_type(
                    own.fixture_state(),
                    target,
                    CardType::Creature
                ));
            }
            let tapped_before = lands
                .iter()
                .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                .count();
            cast(&mut own, &mut native, source);
            assert!(!actions(&own).iter().any(|a| matches!(a, A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == enemy.0)));
            checkpoint(&mut own, &mut native);
            choose(&mut own, &mut native, source, target);
            assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
            assert_eq!(
                own.fixture_state().stack.last().unwrap().is_flashback,
                flashback
            );
            assert_eq!(
                crate::engine::object_mana_value(own.fixture_state(), source),
                6
            );
            assert_eq!(
                lands
                    .iter()
                    .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                    .count()
                    - tapped_before,
                if flashback { 4 } else { 6 }
            );
            assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
            let before = own.fixture_state().players[p.index()].battlefield.clone();
            let life = own.fixture_state().players[p.index()].life;
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            for response in 0..if scene == 0 { 3 } else { 1 } {
                let mut expected = None;
                for _ in 0..2 {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                    if response != 0 {
                        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
                        if response == 1 {
                            cast(&mut own, &mut native, counter);
                            act(&mut own, &mut native, |a| {
                                is_original_spell_target(a, source)
                            });
                        } else {
                            cast(&mut own, &mut native, removal);
                            choose(&mut own, &mut native, removal, target);
                        }
                    }
                    settle(&mut own, &mut native);
                    let state = own.fixture_state();
                    assert_eq!(
                        state.objects.get(source).zone,
                        if flashback {
                            Zone::Exile
                        } else {
                            Zone::Graveyard
                        }
                    );
                    let copies = state.players[p.index()]
                        .battlefield
                        .iter()
                        .copied()
                        .filter(|o| !before.contains(o))
                        .collect::<Vec<_>>();
                    assert_eq!(copies.len(), usize::from(response == 0));
                    if let Some(&copy) = copies.first() {
                        let live = state.objects.get(copy);
                        assert!(live.v4.is_token);
                        assert_eq!((live.owner, live.controller), (p, p));
                        assert!(!live.tapped);
                        assert_eq!(live.damage, 0);
                        assert_eq!(live.counters.plus1_plus1, 0);
                        assert!(live.attachments.is_empty());
                        assert!(live.v4.face_down_v1.is_none());
                        if scene == 3 || scene == 4 {
                            assert_eq!(
                                live.card_def,
                                crate::card_def::card_id_by_name(
                                    crate::self_reflection_v1::ANONYMOUS_BLUEPRINT
                                )
                                .unwrap()
                            );
                            assert!(live.name.is_empty());
                            assert_eq!(
                                crate::engine::checked_effective_pt(state, copy).unwrap(),
                                (2, 2)
                            );
                            assert_eq!(live.v4.ward_generic, 0);
                        } else {
                            assert_eq!(live.card_def, state.objects.get(target).card_def);
                            if scene == 0 {
                                assert_eq!(
                                    crate::engine::checked_effective_pt(state, copy).unwrap(),
                                    (4, 4)
                                );
                            }
                            if scene == 5 {
                                assert!(!crate::engine::object_has_type(
                                    state,
                                    copy,
                                    CardType::Creature
                                ));
                                assert!(crate::engine::object_has_type(
                                    state,
                                    copy,
                                    CardType::Land
                                ));
                            }
                        }
                    }
                    if let Some(creature) = next_creature {
                        assert_eq!(own.fixture_state().players[p.index()].life, life + 1);
                        cast(&mut own, &mut native, creature);
                        settle(&mut own, &mut native);
                        assert_eq!(
                            own.fixture_state().objects.get(creature).zone,
                            Zone::Battlefield
                        );
                    }
                    let state = own.fixture_state();
                    assert_eq!(
                        state.players[p.index()].life,
                        life + if scene == 2 && response == 0 { 3 } else { 0 }
                    );
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
