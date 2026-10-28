use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{actions, cast, choose},
};
use crate::{
    card_def::Keywords,
    effect::{EffectOp, ExecCtx, ObjectRef},
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

#[test]
fn paid_stroke_nonland_menu_current_controller_token_indestructible_counter_illegal_and_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Stroke of Midnight").unwrap();
    assert_eq!(
        super::super::super::roots::registered_spell_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let human = crate::card_def::card_id_by_name("Human Token").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for recipient in [p, p.opponent()] {
            for (name, indestructible, stolen) in [
                ("Llanowar Elves", false, false),
                ("Goldvein Pick", false, false),
                ("Llanowar Elves", true, false),
                ("Llanowar Elves", false, true),
            ] {
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_383,
                    p,
                );
                s.step = Step::Main1;
                for _ in 0..3 {
                    put(&mut s, p, "Plains", Zone::Battlefield);
                    put(&mut s, p.opponent(), "Island", Zone::Battlefield);
                    put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
                }
                let owner = if stolen {
                    recipient.opponent()
                } else {
                    recipient
                };
                let target = put(&mut s, owner, name, Zone::Battlefield);
                // Initial controlled-object fixture, not a paid control spell.
                if stolen {
                    s.players[owner.index()]
                        .battlefield
                        .retain(|&o| o != target);
                    s.players[recipient.index()].battlefield.push(target);
                    s.objects.get_mut(target).controller = recipient;
                }
                if indestructible {
                    // An initial continuous effect tests the printed instruction's
                    // failed-destruction branch; its acquisition is not claimed.
                    crate::effect::execute(
                        &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                            object: ObjectRef::ThisSource,
                            keyword: Keywords::INDESTRUCTIBLE,
                        },
                        &ExecCtx::no_targets(target, recipient),
                        &mut s,
                    );
                }
                let source = put(&mut s, p, "Stroke of Midnight", Zone::Hand);
                let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
                let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
                let (mut own, mut native) = sessions(s);
                cast(&mut own, &mut native, source);
                let menu = actions(&own);
                assert!(menu.iter().any(|a| matches!(a,
                    A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
                    if object.arena_id == target.0)));
                for a in &menu {
                    if let A::ChooseTarget {
                        target: crate::rl::TargetRefV1::Object { object },
                        ..
                    } = a
                    {
                        assert!(!crate::engine::object_has_type(
                            own.fixture_state(),
                            crate::ids::ObjectId(object.arena_id),
                            crate::card_def::CardType::Land,
                        ));
                    }
                }
                checkpoint(&mut own, &mut native);
                choose(&mut own, &mut native, source, target);
                assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
                assert_eq!(
                    crate::engine::object_mana_value(own.fixture_state(), source),
                    3
                );
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .battlefield
                        .iter()
                        .filter(|&&o| own.fixture_state().objects.get(o).tapped)
                        .count(),
                    3
                );
                let paid = own.snapshot();
                let raw = native.snapshot_v1();
                for response in 0..3 {
                    if response == 2 && (name != "Llanowar Elves" || indestructible) {
                        continue;
                    }
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&paid).unwrap();
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
                        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                        assert_eq!(state.objects.get(target).owner, owner);
                        assert_eq!(
                            state.objects.get(target).zone,
                            if response == 1 || indestructible {
                                Zone::Battlefield
                            } else {
                                Zone::Graveyard
                            }
                        );
                        for player in [p, p.opponent()] {
                            let tokens = state.players[player.index()]
                                .battlefield
                                .iter()
                                .copied()
                                .filter(|&o| state.objects.get(o).card_def == human)
                                .collect::<Vec<_>>();
                            assert_eq!(
                                tokens.len(),
                                usize::from(response == 0 && player == recipient)
                            );
                            for token in tokens {
                                let object = state.objects.get(token);
                                assert_eq!(object.owner, recipient);
                                assert_eq!(object.controller, recipient);
                                assert!(!object.tapped);
                                assert!(object.summoning_sick);
                                assert_eq!(
                                    crate::engine::checked_effective_pt(state, token).unwrap(),
                                    (1, 1)
                                );
                            }
                        }
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
}
