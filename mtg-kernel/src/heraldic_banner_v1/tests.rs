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
    for def in CARD_DEFS.iter() {
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
                    branch.players[actor.index()].mana_pool[color.pool_index()] = u16::MAX;
                    assert!(
                        engine::available_mana_ability_choices(actor, banner, branch).is_empty()
                    );
                    let full = serde_json::to_vec(branch).unwrap();
                    assert!(engine::step(branch, Action::ActivateManaAbility(banner)).is_err());
                    assert_eq!(serde_json::to_vec(branch).unwrap(), full);
                    branch.players[actor.index()].mana_pool[color.pool_index()] = 0;
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
            let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
            let first =
                crate::banner_observation_v1::observe_v1(&state, &surface, actor, 0, 0, 0, 1)
                    .unwrap();
            assert!(first.prior_entry_colors.is_empty());
            engine::step(&mut state, Action::ChooseEffectOption(4)).unwrap();
            assert_eq!(reach_color(&mut state), actor.opponent());
            let later = crate::banner_observation_v1::observe_v1(
                &state,
                &surface,
                actor.opponent(),
                0,
                0,
                0,
                1,
            )
            .unwrap();
            assert_eq!(
                later.prior_entry_colors,
                vec![crate::banner_observation_v1::PriorEntryColorV36 {
                    entry_ordinal: 1,
                    chooser: actor.into(),
                    printed_card_db_id: state.objects.get(own).card_def,
                    chosen_color: ManaColor::G,
                }]
            );
            assert_eq!(later.entry_choice.as_ref().unwrap().entry_ordinal, 0);
            assert_eq!(
                later.entry_choice.as_ref().unwrap().chooser,
                actor.opponent().into()
            );
            let answer = serde_json::to_value(&later.prior_entry_colors).unwrap();
            assert_eq!(answer[0].as_object().unwrap().len(), 4);
            assert!(answer[0].get("original").is_none() && answer[0].get("arena_id").is_none());
            for id in [own, opponent] {
                assert_eq!(state.objects.get(id).zone, Zone::Hand);
            }
            let mut altered = state.clone();
            let pending = altered
                .engine
                .linked_aura_operation_v1
                .pending_general_v36
                .as_mut()
                .unwrap();
            let crate::linked_aura_operation_v1::GeneralAnswerV36::Color { color, .. } =
                &mut pending.answers[0]
            else {
                panic!("actual prior color answer");
            };
            *color = ManaColor::U;
            let before = serde_json::to_vec(&altered).unwrap();
            assert!(crate::banner_observation_v1::observe_v1(
                &altered,
                &surface,
                actor.opponent(),
                0,
                0,
                0,
                1
            )
            .is_err());
            assert!(engine::step(&mut altered, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&altered).unwrap(), before);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                let actual = crate::banner_observation_v1::observe_v1(
                    branch,
                    &surface,
                    actor.opponent(),
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                assert_eq!(actual.prior_entry_colors, later.prior_entry_colors);
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
    #[test]

    fn actual_copy_allocator_owns_fresh_token_color_before_allocation_and_restores_tail() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(banner, Zone::Battlefield),
            );
            engine::step(&mut state, Action::ChooseEffectOption(4)).unwrap();
            let before = state.objects.len();
            // Genuine accepted copy allocator, not a claimed printed FDN
            // spell which copies arbitrary artifacts.
            assert!(crate::standard_cards_v1::create_token_copy(
                &mut state,
                banner,
                actor.opponent()
            )
            .is_none());
            assert_eq!(reach_color(&mut state), actor.opponent());
            assert_eq!(state.objects.len(), before);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                engine::step(branch, Action::ChooseEffectOption(3)).unwrap();
                let token = branch
                    .objects
                    .iter()
                    .find_map(|(id, o)| {
                        (id != banner
                            && owns_definition(o.card_def)
                            && o.zone == Zone::Battlefield
                            && o.controller == actor.opponent())
                        .then_some(id)
                    })
                    .unwrap();
                assert!(branch.objects.get(token).v4.is_token);
                assert_eq!(branch.objects.len(), before + 1);
                assert_eq!(authenticated_color(branch, banner).unwrap(), ManaColor::G);
                assert_eq!(authenticated_color(branch, token).unwrap(), ManaColor::R);
                assert_eq!(branch.objects.get(token).owner, actor.opponent());
                validate_state(branch).unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
    #[test]

    fn actual_short_payment_countered_spell_and_effective_recipient_color_recheck_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::CastSpell(banner)).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
            let counter = put(&mut state, actor.opponent(), "Counterspell", Zone::Hand);
            state.players[actor.opponent().index()].mana_pool[ManaColor::U.pool_index()] = 2;
            engine::step(&mut state, Action::CastSpell(banner)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            for _ in 0..4 {
                if state.priority_player == actor.opponent() {
                    break;
                }
                engine::step(&mut state, Action::Pass).unwrap();
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::CastSpellOrPass { .. }
                ));
            }
            assert_eq!(state.priority_player, actor.opponent());
            engine::step(&mut state, Action::CastSpell(counter)).unwrap();
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { legal_targets, .. } if legal_targets == [crate::state::Target::Object(banner)]));
            engine::step(
                &mut state,
                Action::ChooseTarget(crate::state::Target::Object(banner)),
            )
            .unwrap();
            for _ in 0..12 {
                let d = engine::advance_until_decision(&mut state);
                assert!(
                    !matches!(d, Decision::ChooseEntryColorV36 { .. }),
                    "countered spell never enters"
                );
                if state.objects.get(banner).zone == Zone::Graveyard {
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert_eq!(state.objects.get(banner).zone, Zone::Graveyard);
            assert!(!state
                .engine
                .event_history
                .iter()
                .any(|e| matches!(e, CommittedEvent::BannerEntryColorCapturedV1 { .. })));
            validate_state(&state).unwrap();
            let mut state = ready(actor);
            let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
            let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(banner, Zone::Battlefield),
            );
            engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
            assert_eq!(engine::effective_power(&state, elf), 1);
            let witness = put(&mut state, actor, "Witness Protection", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(witness)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            engine::step(
                &mut state,
                Action::ChooseTarget(crate::state::Target::Object(elf)),
            )
            .unwrap();
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                for _ in 0..12 {
                    let d = engine::advance_until_decision(branch);
                    if branch.objects.get(witness).zone == Zone::Battlefield {
                        break;
                    }
                    assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                    engine::step(branch, Action::Pass).unwrap();
                }
                assert_eq!(
                    engine::object_color_mask(branch, elf),
                    crate::card_def::mana_color_mask(ManaColor::G)
                        | crate::card_def::mana_color_mask(ManaColor::W)
                );
                // Mage's Witness makes a green and white Citizen 1/1, so the
                // chosen white anthem becomes applicable to its new colors.
                assert_eq!(engine::effective_power(branch, elf), 2);
                assert_eq!(authenticated_color(branch, banner).unwrap(), ManaColor::W);
                validate_state(branch).unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
