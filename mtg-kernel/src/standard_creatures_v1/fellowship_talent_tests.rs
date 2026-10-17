//! Genuine Standard Talent cast/level payments, with explicitly native
//! fellowship placement/quote primitives. No Banner definition is fabricated.
use super::*;
use crate::{
    banner_of_kinship_v1::EntryChoiceV1,
    creature_types_v60::CreatureTypeV60,
    effect::EffectOp,
    engine::{self, Action, Decision},
    event::ProposedEvent,
    ids::PlayerId,
    policy_observation_v6::tests::{put, ready_state_for},
    state::{Counters, Target},
};

fn settle(state: &mut GameState) {
    for _ in 0..32 {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("genuine Talent resolution: {other:?}"),
        }
    }
    panic!("genuine Talent resolution did not settle");
}

fn paid_talents(actor: PlayerId, count: usize, level: u8, reserve: usize) -> (GameState, ObjectId) {
    let mut state = ready_state_for(actor);
    let per_talent = 2 + usize::from(level >= 2) + 4 * usize::from(level >= 3);
    let forests = (0..1 + per_talent * count + reserve)
        .map(|_| put(&mut state, actor, "Forest", Zone::Battlefield))
        .collect::<Vec<_>>();
    let elf = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
    engine::step(&mut state, Action::CastSpell(elf)).unwrap();
    settle(&mut state);
    assert_eq!(state.objects.get(elf).zone, Zone::Battlefield);
    for _ in 0..count {
        let talent = put(&mut state, actor, "Innkeeper's Talent", Zone::Hand);
        engine::step(&mut state, Action::CastSpell(talent)).unwrap();
        settle(&mut state);
        assert_eq!(crate::standard_cards_v1::class_level(&state, talent), 1);
        for (ability, expected) in [(0, 2), (1, 3)] {
            if expected <= level {
                engine::step(&mut state, Action::ActivateAbility(talent, ability)).unwrap();
                settle(&mut state);
                assert_eq!(
                    crate::standard_cards_v1::class_level(&state, talent),
                    expected
                );
            }
        }
    }
    assert_eq!(
        forests
            .iter()
            .filter(|id| state.objects.get(**id).tapped)
            .count(),
        1 + per_talent * count
    );
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    assert!(!crate::doubling_season_v48::requires_successor(&state));
    (state, elf)
}

#[test]
fn paid_talent_level_two_gives_fellowship_only_permanent_real_ward_one_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, elf) = paid_talents(actor, 1, 2, 0);
        // Native counter-storage primitive. The Elf has no invented ability.
        for amount in [0, -1, 1] {
            state.objects.get_mut(elf).counters.fellowship = amount;
            assert_eq!(
                crate::standard_cards_v1::granted_wards(&state, elf),
                if amount > 0 { vec![1] } else { vec![] }
            );
            if amount != 0 {
                assert!(
                    crate::rl::observe_v1(&state, actor, 0)
                        .unwrap_err()
                        .to_string()
                        .contains("fellowship")
                );
            }
        }
        assert_eq!(
            state.objects.get(elf).counters,
            Counters {
                fellowship: 1,
                ..Default::default()
            }
        );
        put(&mut state, actor.opponent(), "Mountain", Zone::Battlefield);
        let bolt = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
        );
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(elf))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert!(state.objects.get(bolt).zone == Zone::Stack);
        assert_eq!(state.players[actor.opponent().index()].mana_pool, [0; 6]);
        let wards = state
            .stack
            .iter()
            .filter(|item| {
                item.source == elf
                    && matches!(
                        item.inline_effect.as_ref(),
                        Some(EffectOp::CounterUnlessPaysGeneric { generic: 1, .. })
                    )
            })
            .collect::<Vec<_>>();
        assert_eq!(wards.len(), 1);
        assert_eq!(wards[0].controller, actor);
        engine::validated_stack_item_target_spec(wards[0], &state).unwrap();
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
        for item in &restored.stack {
            engine::validated_stack_item_target_spec(item, &restored).unwrap();
        }
    }
}

#[test]
fn paid_talent_without_season_checked_fellowship_transfer_rejects_overflow_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, elf) = paid_talents(actor, 1, 3, 0);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            assert_eq!(
                crate::standard_cards_v1::counter_modifier_doublings_v48(branch, actor),
                1
            );
            for amount in [i32::MAX, -1] {
                let before = branch.clone();
                let transfer = CounterTransferV1 {
                    counters: Counters {
                        fellowship: amount,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                assert!(apply_counter_transfer(branch, elf, actor, transfer).is_none());
                assert_eq!(*branch, before);
            }
            let transfer = CounterTransferV1 {
                counters: Counters {
                    fellowship: 3,
                    ..Default::default()
                },
                ..Default::default()
            };
            apply_counter_transfer(branch, elf, actor, transfer).unwrap();
            assert_eq!(branch.objects.get(elf).counters.fellowship, 6);
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn paid_talents_without_season_checked_fellowship_entry_quote_doubles_or_refuses_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for count in [0, 1, 31] {
            // All level-three modifiers are genuine, individually paid Classes.
            let (mut state, _) = paid_talents(actor, count, 3, 1);
            let entering = put(&mut state, actor, "Gingerbrute", Zone::Hand);
            engine::step(&mut state, Action::CastSpell(entering)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            assert_eq!(state.objects.get(entering).zone, Zone::Stack);
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            // A native entry proposal/quote primitive for a genuinely paid
            // spell. Gingerbrute gains no type choice or fellowship ability.
            let proposal = ProposedEvent::zone_change(entering, Zone::Battlefield);
            let before = state.clone();
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&state, &restored] {
                let prepared = EntryChoiceV1::capture(branch, &proposal).unwrap();
                let result = prepared.answer(branch, CreatureTypeV60::Elf.choice_index());
                if count == 31 {
                    assert!(result.is_err());
                } else {
                    let result = result.unwrap();
                    assert_eq!(result.before_replacements, 1);
                    assert_eq!(result.after_replacements, if count == 0 { 1 } else { 2 });
                }
                assert_eq!(
                    prepared
                        .answer(branch, CreatureTypeV60::Bear.choice_index())
                        .unwrap()
                        .after_replacements,
                    0
                );
            }
            assert_eq!(state, before);
            assert_eq!(restored, before);
        }
    }
}
