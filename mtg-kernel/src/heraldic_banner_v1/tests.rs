use super::*;
use crate::engine::{self, Action, Decision};
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn registered_absence_retains_original_linked_state_bytes_and_gate_semantics() {
    let state = ready_state();
    assert_eq!(
        serde_json::to_value(&state.engine.linked_aura_operation_v1).unwrap(),
        serde_json::json!({"pending": null})
    );
    for def in CARD_DEFS {
        if def.as_enters_choose_color_other_than.is_some() {
            assert_eq!(def.as_enters_color_choice_v1, None);
            assert_eq!(def.chosen_color_creature_boost_v1, None);
        }
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        let land = put(&mut state, actor, "Forest", Zone::Hand);
        let mut original = state.clone();
        engine::step(&mut state, Action::PlayLand(land)).unwrap();
        engine::step_without_linked_aura_v1(&mut original, Action::PlayLand(land)).unwrap();
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&original).unwrap()
        );
        validate_state(&state).unwrap();
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    fn ready(actor: PlayerId) -> GameState {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 16],
            &[forest; 16],
            |id| CARD_DEFS[id as usize].name.into(),
            11036,
            actor,
        );
        state.step = crate::state::Step::Main1;
        state
    }
    fn reach_color(state: &mut GameState) -> PlayerId {
        for _ in 0..16 {
            match engine::advance_until_decision(state) {
                Decision::ChooseEntryColorV36 { player, colors, .. } => {
                    assert_eq!(colors, COLORS);
                    return player;
                }
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("actual entry color {other:?}"),
            }
        }
        panic!("actual color menu missing")
    }
    #[test]
    #[ignore = "requires genuine Heraldic Banner registration; no fabricated root"]
    fn real_cast_preentry_five_colors_restore_anthem_and_immediate_mana_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for (index, color) in COLORS.into_iter().enumerate() {
                let mut state = ready(actor);
                let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
                state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 3;
                engine::step(&mut state, Action::CastSpell(banner)).unwrap();
                assert_eq!(reach_color(&mut state), actor);
                assert_eq!(state.objects.get(banner).zone, Zone::Stack);
                let bytes = serde_json::to_vec(&state).unwrap();
                let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
                for branch in [&mut state, &mut restored] {
                    assert!(engine::step(branch, Action::Pass).is_err());
                    assert_eq!(serde_json::to_vec(branch).unwrap(), bytes);
                    assert!(engine::step(branch, Action::ChooseEffectOption(5)).is_err());
                    assert_eq!(serde_json::to_vec(branch).unwrap(), bytes);
                    engine::step(branch, Action::ChooseEffectOption(index as u16)).unwrap();
                    assert_eq!(branch.objects.get(banner).zone, Zone::Battlefield);
                    assert_eq!(authenticated_color(branch, banner).unwrap(), color);
                    validate_state(branch).unwrap();
                    assert_eq!(branch.players[actor.index()].mana_pool, [0; 6]);
                    engine::step(branch, Action::ActivateManaAbility(banner)).unwrap();
                    assert!(branch.objects.get(banner).tapped);
                    assert_eq!(
                        branch.players[actor.index()].mana_pool[color.pool_index()],
                        1
                    );
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
            }
        }
    }
    #[test]
    #[ignore = "requires genuine Heraldic Banner registration; no fabricated root"]
    fn true_simultaneous_entry_choices_precede_both_entries_and_follow_apnap_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let opponent = put(&mut state, actor.opponent(), "Heraldic Banner", Zone::Hand);
            let own = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
            crate::event::propose_and_commit_batch(
                &mut state,
                vec![
                    ProposedEvent::zone_change(opponent, Zone::Battlefield),
                    ProposedEvent::zone_change(own, Zone::Battlefield),
                ],
            );
            assert_eq!(reach_color(&mut state), actor);
            for id in [own, opponent] {
                assert_eq!(state.objects.get(id).zone, Zone::Hand);
            }
            engine::step(&mut state, Action::ChooseEffectOption(4)).unwrap();
            assert_eq!(reach_color(&mut state), actor.opponent());
            for id in [own, opponent] {
                assert_eq!(state.objects.get(id).zone, Zone::Hand);
            }
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                engine::step(branch, Action::ChooseEffectOption(3)).unwrap();
                assert_eq!(authenticated_color(branch, own).unwrap(), ManaColor::G);
                assert_eq!(authenticated_color(branch, opponent).unwrap(), ManaColor::R);
                validate_state(branch).unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
    #[test]
    #[ignore = "requires genuine Heraldic Banner registration; no fabricated root"]
    fn erased_rewritten_entry_checkpoint_refuses_readonly_and_new_entry_needs_new_color() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(banner, Zone::Battlefield),
            );
            assert_eq!(reach_color(&mut state), actor);
            for mode in 0..3 {
                let mut bad = state.clone();
                match mode {
                    0 => bad.engine.linked_aura_operation_v1.pending_general_v36 = None,
                    1 => bad
                        .engine
                        .linked_aura_operation_v1
                        .pending_general_v36
                        .as_mut()
                        .unwrap()
                        .answers
                        .push(crate::linked_aura_operation_v1::GeneralAnswerV36::Color {
                            choice: choice(
                                &state,
                                &ProposedEvent::zone_change(banner, Zone::Battlefield),
                                0,
                            )
                            .unwrap()
                            .unwrap(),
                            color: ManaColor::C,
                        }),
                    _ => bad.engine.event_history.retain(|e| {
                        !matches!(e, CommittedEvent::BannerEntryColorCapturedV1 { .. })
                    }),
                }
                let mut bad: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&bad).unwrap()).unwrap();
                let bytes = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            }
            engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(banner, Zone::Hand),
            );
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(banner, Zone::Battlefield),
            );
            assert_eq!(reach_color(&mut state), actor);
            engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
            assert_eq!(authenticated_color(&state, banner).unwrap(), ManaColor::U);
            validate_state(&state).unwrap();
        }
    }
}
