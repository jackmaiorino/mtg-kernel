use super::*;
use super::{
    surveil_many::{checkpoint, sessions},
    targeted_entry::{cast, choose},
};
use crate::{
    card_def::Keywords,
    ids::ObjectId,
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};

fn through_combat(
    own: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    attacker: ObjectId,
    attacked: bool,
) {
    for _ in 0..256 {
        if own.fixture_state().step == Step::Main2 {
            return;
        }
        act(own, native, |a| match a {
            A::ChooseAttackerInclusion {
                attacker: a,
                include,
                ..
            } => *include == (attacked && a.arena_id == attacker.0),
            A::DeclareAttackers { attackers, .. } => {
                attackers.len() == usize::from(attacked)
                    && attackers.iter().all(|a| a.arena_id == attacker.0)
            }
            A::ChooseBlockerInclusion { include, .. } => !*include,
            A::DeclareBlockersForAttacker { blockers, .. } => blockers.is_empty(),
            A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. } => true,
            _ => false,
        });
    }
    panic!("Searslicer scene did not finish actual combat");
}
#[test]
fn paid_searslicer_before_or_after_actual_attack_own_end_step_raid_source_and_attacker_departure_restore_both_seats(
) {
    let id = crate::card_def::card_id_by_name("Searslicer Goblin").unwrap();
    assert_eq!(
        super::super::super::roots::registered_permanent_printing(id).unwrap(),
        Some(super::super::super::roots::baseline(id).unwrap())
    );
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let goblin = crate::card_def::card_id_by_name("Goblin Token").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        for attacked in [false, true] {
            for late in [false, true] {
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_398,
                    p,
                );
                s.step = Step::Main1;
                let lands = (0..2)
                    .map(|_| put(&mut s, p, "Mountain", Zone::Battlefield))
                    .collect::<Vec<_>>();
                for name in ["Island", "Swamp"] {
                    for _ in 0..3 {
                        put(&mut s, p.opponent(), name, Zone::Battlefield);
                    }
                }
                let attacker = put(&mut s, p, "Savannah Lions", Zone::Battlefield);
                let source = put(&mut s, p, "Searslicer Goblin", Zone::Hand);
                let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
                let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
                let (mut own, mut native) = sessions(s);
                if late {
                    through_combat(&mut own, &mut native, attacker, attacked);
                }
                cast(&mut own, &mut native, source);
                assert_eq!(own.fixture_state().objects.get(source).zone, Zone::Stack);
                assert_eq!(
                    crate::engine::object_mana_value(own.fixture_state(), source),
                    2
                );
                assert!(lands
                    .iter()
                    .all(|&o| own.fixture_state().objects.get(o).tapped));
                assert_eq!(own.fixture_state().players[p.index()].mana_pool, [0; 6]);
                if !late && !attacked {
                    let saved = own.snapshot();
                    let raw = native.snapshot_v1();
                    cast(&mut own, &mut native, counter);
                    act(&mut own, &mut native, |a| {
                        is_original_spell_target(a, source)
                    });
                    settle(&mut own, &mut native);
                    assert_eq!(
                        own.fixture_state().objects.get(source).zone,
                        Zone::Graveyard
                    );
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                }
                settle(&mut own, &mut native);
                if !late {
                    through_combat(&mut own, &mut native, attacker, attacked);
                }
                assert_eq!(
                    own.fixture_state().player_attacked_this_turn_v1(p),
                    attacked
                );
                assert!(own.fixture_state().engine.combat.attackers.is_empty());
                for _ in 0..64 {
                    if own.fixture_state().step == Step::End
                        && own.fixture_state().engine.pending_triggers.is_empty()
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
                assert_eq!(own.fixture_state().step, Step::End);
                assert_eq!(
                    own.fixture_state()
                        .stack
                        .iter()
                        .any(|i| i.source == source && i.kind == StackItemKind::TriggeredAbility),
                    attacked
                );
                checkpoint(&mut own, &mut native);
                let captured = own.snapshot();
                let raw = native.snapshot_v1();
                for response in 0..if attacked { 3 } else { 1 } {
                    let mut expected = None;
                    for _ in 0..2 {
                        own.restore(&captured).unwrap();
                        native.kaito_restore_v75(&raw).unwrap();
                        if response != 0 {
                            cast(&mut own, &mut native, removal);
                            choose(
                                &mut own,
                                &mut native,
                                removal,
                                if response == 1 { source } else { attacker },
                            );
                        }
                        settle(&mut own, &mut native);
                        let state = own.fixture_state();
                        let tokens = state.players[p.index()]
                            .battlefield
                            .iter()
                            .copied()
                            .filter(|&o| state.objects.get(o).card_def == goblin)
                            .collect::<Vec<_>>();
                        assert_eq!(tokens.len(), usize::from(attacked));
                        for token in tokens {
                            let o = state.objects.get(token);
                            assert_eq!((o.owner, o.controller), (p, p));
                            assert!(o.v4.is_token && o.summoning_sick);
                            assert!(!o.tapped);
                            assert!(!state.engine.combat.attackers.contains(&token));
                            assert!(!crate::engine::has_effective_keyword(
                                state,
                                token,
                                Keywords::HASTE
                            ));
                            assert_eq!(
                                crate::engine::checked_effective_pt(state, token).unwrap(),
                                (1, 1)
                            );
                        }
                        assert_eq!(
                            state.objects.get(source).zone,
                            if response == 1 {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        assert_eq!(
                            state.objects.get(attacker).zone,
                            if response == 2 {
                                Zone::Graveyard
                            } else {
                                Zone::Battlefield
                            }
                        );
                        if let Some(ref old) = expected {
                            assert_eq!(state, old);
                        } else {
                            expected = Some(state.clone());
                        }
                        checkpoint(&mut own, &mut native);
                    }
                }
            }
        }
    }
}
