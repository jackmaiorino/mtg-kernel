use super::*;
use crate::event::{self, ProposedEvent};
use crate::policy_observation_v6::tests::{put, ready_state_for};

fn bytes(state: &GameState) -> Vec<u8> {
    serde_json::to_vec(state).unwrap()
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&bytes(state)).unwrap()
}

#[test]
fn genuine_counter_primitive_has_no_new_absent_source_state_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let creature = put(&mut state, actor, "Forest", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        assert!(!requires_successor(&state));
        let before = bytes(&state);
        let captured = ReplacementInputV48::capture(&state, actor);
        assert!(captured.seasons.is_empty());
        assert_eq!(captured.replaced_count(3).unwrap(), 3);
        assert_eq!(bytes(&state), before);
        for origin in [
            CounterOriginV48::Effect,
            CounterOriginV48::Cost,
            CounterOriginV48::Rule,
        ] {
            assert_eq!(
                counter_amount(&state, creature, actor.opponent(), origin, 3).unwrap(),
                3
            );
        }
        event::add_plus_one_counters(&mut state, creature, actor.opponent(), 3).unwrap();
        assert_eq!(state.objects.get(creature).counters.plus1_plus1, 3);
        assert_eq!(restore(&state), state);
        let before = bytes(&state);
        assert!(event::add_plus_one_counters(&mut state, creature, actor, i32::MAX).is_err());
        assert_eq!(bytes(&state), before);
    }
}

#[test]
fn ordinary_token_proposal_and_real_batch_keep_absent_extension_bytes() {
    let token = crate::card_def::CARD_DEFS
        .iter()
        .position(|d| d.is_token && d.has_full_support())
        .unwrap() as u16;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let proposal = ProposedEvent::create_token(token, actor);
        let value = serde_json::to_value(&proposal).unwrap();
        assert!(value["CreateToken"].get("entry_tapped").is_none());
        let original = vec![proposal.clone(), proposal];
        let (expanded, receipt) = expand_tokens(&state, original.clone()).unwrap();
        assert_eq!(expanded, original);
        assert!(receipt.is_empty());
        let start = state.engine.event_history.len();
        event::propose_and_commit_batch(&mut state, expanded);
        assert_eq!(
            state.engine.event_history[start..]
                .iter()
                .filter(|e| matches!(e, event::CommittedEvent::CreateToken { .. }))
                .count(),
            2
        );
        assert!(!state.engine.event_history.iter().any(|e| matches!(
            e,
            event::CommittedEvent::TokenCreationReplacementV48 { .. }
                | event::CommittedEvent::PlusCounterReplacementV48 { .. }
        )));
        validate_token_journals(&restore(&state)).unwrap();
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_simultaneous_expanded_roster_original_control_and_private_receipt_refusal() {
    use crate::engine::{self, Action};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for p in [actor, actor.opponent()] {
            put(&mut state, p, "Forest", Zone::Battlefield);
            put(&mut state, p, "Forest", Zone::Library);
        }
        let season = cast_season(&mut state, actor);
        let token = crate::card_def::card_id_by_name("Cat Token").unwrap();
        let mut original = ProposedEvent::create_token(token, actor);
        if let ProposedEvent::CreateToken(t) = &mut original {
            t.entry_tapped = true;
        }
        let start = state.engine.event_history.len();
        event::propose_and_commit_batch(
            &mut state,
            vec![
                original.clone(),
                original,
                ProposedEvent::create_token(token, actor.opponent()),
            ],
        );
        validate_token_journals(&state).unwrap();
        assert_eq!(
            expected_tokens(
                &state,
                start,
                state.engine.event_history.len(),
                token,
                actor,
                2
            )
            .unwrap(),
            4
        );
        let created = state.engine.event_history[start..]
            .iter()
            .filter_map(|e| match e {
                event::CommittedEvent::CreateToken {
                    object, controller, ..
                } => Some((*object, *controller)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 5);
        assert!(created[..4]
            .iter()
            .all(|(id, p)| *p == actor && state.objects.get(*id).tapped));
        assert_eq!(created[4].1, actor.opponent());
        assert!(!state.objects.get(created[4].0).tapped);
        crate::standard_cards_v1::gain_control(&mut state, season, actor.opponent());
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Graveyard),
        );
        let stable = restore(&state);
        validate_token_journals(&stable).unwrap();
        assert_eq!(
            expected_tokens(
                &stable,
                start,
                stable.engine.event_history.len(),
                token,
                actor,
                2
            )
            .unwrap(),
            4
        );
        for _ in 0..3 {
            let mut forged = restore(&stable);
            let marker = forged
                .engine
                .event_history
                .iter()
                .find(|e| matches!(e, event::CommittedEvent::TokenCreationReplacementV48 { .. }))
                .unwrap()
                .clone();
            forged.engine.event_log.push(marker);
            let before = bytes(&forged);
            assert!(engine::step(&mut forged, Action::Pass).is_err());
            assert_eq!(bytes(&forged), before);
            let mut forged = restore(&stable);
            let event::CommittedEvent::TokenCreationReplacementV48 { creation } = forged
                .engine
                .event_history
                .iter_mut()
                .find(|e| matches!(e, event::CommittedEvent::TokenCreationReplacementV48 { .. }))
                .unwrap()
            else {
                unreachable!()
            };
            creation.created.pop();
            let before = bytes(&forged);
            assert!(engine::step(&mut forged, Action::Pass).is_err());
            assert_eq!(bytes(&forged), before);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_simultaneous_season_entry_does_not_depend_on_member_order() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for source_first in [false, true] {
            let mut state = ready_state_for(actor);
            let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
            let hydra = put(&mut state, actor, "Mossborn Hydra", Zone::Hand);
            let season_entry = ProposedEvent::zone_change(season, Zone::Battlefield);
            let hydra_entry = ProposedEvent::zone_change(hydra, Zone::Battlefield);
            event::propose_and_commit_batch(
                &mut state,
                if source_first {
                    vec![season_entry, hydra_entry]
                } else {
                    vec![hydra_entry, season_entry]
                },
            );
            assert!(state.engine.halted.is_none());
            assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 1);
            let later = put(&mut state, actor, "Mossborn Hydra", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(later, Zone::Battlefield),
            );
            assert_eq!(state.objects.get(later).counters.plus1_plus1, 2);
            let opponent = put(&mut state, actor.opponent(), "Mossborn Hydra", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(opponent, Zone::Battlefield),
            );
            assert_eq!(state.objects.get(opponent).counters.plus1_plus1, 1);
            assert_eq!(restore(&state), state);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_ajani_entry_eight_and_genuine_zero_target_plus_cost_one() {
    use crate::engine::{self, Action, Decision};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for p in [actor, actor.opponent()] {
            put(&mut state, p, "Forest", Zone::Battlefield);
            put(&mut state, p, "Forest", Zone::Library);
        }
        cast_season(&mut state, actor);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        let ajani = put(&mut state, actor, "Ajani, Caller of the Pride", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 3;
        engine::step(&mut state, Action::CastSpell(ajani)).unwrap();
        for _ in 0..32 {
            let d = engine::advance_until_decision(&mut state);
            assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
            if state.objects.get(ajani).zone == Zone::Battlefield {
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(crate::planeswalker_v1::loyalty(&state, ajani), Some(8));
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            assert!(
                matches!(engine::advance_until_decision(branch),Decision::CastSpellOrPass {player,..} if player==actor)
            );
            engine::step(branch, Action::ActivateAbility(ajani, 0)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(branch),
                Decision::ChooseEffectTargets { .. }
            ));
            engine::step(branch, Action::FinishEffectSelection).unwrap();
            assert!(matches!(
                engine::advance_until_decision(branch),
                Decision::CastSpellOrPass { .. }
            ));
            assert!(branch.engine.pending_activation.is_none());
            assert_eq!(branch.stack.last().unwrap().targets.len(), 0);
            assert_eq!(crate::planeswalker_v1::loyalty(branch, ajani), Some(9));
            while !branch.stack.is_empty() {
                assert!(matches!(
                    engine::advance_until_decision(branch),
                    Decision::CastSpellOrPass { .. }
                ));
                engine::step(branch, Action::Pass).unwrap();
            }
            crate::ajani_full_v42::lifecycle::validate_state(branch).unwrap();
            assert_eq!(crate::planeswalker_v1::loyalty(branch, ajani), Some(9));
        }
        assert_eq!(state, restored);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn cast_season(state: &mut GameState, actor: PlayerId) -> ObjectId {
    use crate::engine::{self, Action, Decision};
    let source = put(state, actor, "Doubling Season", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(state),Decision::CastSpellOrPass{player,..} if player==actor)
    );
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    for _ in 0..32 {
        if state.objects.get(source).zone == Zone::Battlefield {
            return source;
        }
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual Season spell did not resolve");
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_printed_source_recipient_controller_multiple_sources_cost_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for p in [actor, actor.opponent()] {
            put(&mut state, p, "Forest", Zone::Battlefield);
            put(&mut state, p, "Forest", Zone::Library);
        }
        let first = cast_season(&mut state, actor);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        assert_eq!(
            counter_amount(
                &state,
                creature,
                actor.opponent(),
                CounterOriginV48::Effect,
                1
            )
            .unwrap(),
            2
        );
        assert_eq!(
            counter_amount(&state, creature, actor, CounterOriginV48::Cost, 1).unwrap(),
            1
        );
        let mut restored = restore(&state);
        event::add_plus_one_counters(&mut restored, creature, actor.opponent(), 2).unwrap();
        assert_eq!(restored.objects.get(creature).counters.plus1_plus1, 4);
        crate::standard_cards_v1::gain_control(&mut restored, first, actor.opponent());
        assert_eq!(
            counter_amount(&restored, creature, actor, CounterOriginV48::Effect, 1).unwrap(),
            1
        );
        crate::standard_cards_v1::gain_control(&mut restored, creature, actor.opponent());
        assert_eq!(
            counter_amount(&restored, creature, actor, CounterOriginV48::Effect, 1).unwrap(),
            2
        );
        event::propose_and_commit(
            &mut restored,
            ProposedEvent::zone_change(first, Zone::Graveyard),
        );
        assert_eq!(
            counter_amount(&restored, creature, actor, CounterOriginV48::Effect, 1).unwrap(),
            1
        );
        // Actual second printed permanent enters by the accepted effect entry.
        let second = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(second, Zone::Battlefield),
        );
        assert_eq!(sources(&state, actor).len(), 2);
        event::add_plus_one_counters(&mut state, creature, actor.opponent(), 1).unwrap();
        assert_eq!(state.objects.get(creature).counters.plus1_plus1, 4);
        let before = bytes(&state);
        assert!(event::add_plus_one_counters(&mut state, creature, actor, i32::MAX).is_err());
        assert_eq!(bytes(&state), before);
        assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
        assert_eq!(bytes(&state), before);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_effect_lore_and_rule_lore_are_distinct_counter_placements() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Battlefield),
        );
        let recipient = put(&mut state, actor, "Forest", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(recipient, Zone::Battlefield),
        );
        event::add_lore_counters(&mut state, recipient, 1).unwrap();
        assert_eq!(state.objects.get(recipient).counters.lore, 2);
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            event::add_lore_counters_rule(branch, recipient, 1).unwrap();
            assert_eq!(branch.objects.get(recipient).counters.lore, 3);
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn genuine_initialized_token_batch_haste_and_copy_are_present_at_entry() {
    let token = CARD_DEFS
        .iter()
        .position(|d| d.is_token && d.has_full_support() && d.types.contains(&CardType::Creature))
        .unwrap() as u16;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let mut proposal = ProposedEvent::create_token(token, actor);
        if let ProposedEvent::CreateToken(token) = &mut proposal {
            token.entry_tapped = true;
            token.initialization_v48 = Some(TokenEntryV48 {
                copied: true,
                haste_until_end_of_turn: true,
                plus_one_counters: 3,
                ..Default::default()
            });
        }
        let start = state.engine.event_history.len();
        event::propose_and_commit_batch(&mut state, vec![proposal.clone(), proposal]);
        let created = state.engine.event_history[start..]
            .iter()
            .filter_map(|event| match event {
                event::CommittedEvent::CreateToken { object, .. } => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 2);
        for object in created {
            assert!(state.objects.get(object).tapped);
            assert!(state.objects.get(object).v4.is_token);
            assert_eq!(state.objects.get(object).counters.plus1_plus1, 3);
            assert!(crate::engine::has_effective_keyword(
                &state,
                object,
                Keywords::HASTE
            ));
            assert_eq!(state.engine.event_history[start..].iter().filter(|event| matches!(event,
                event::CommittedEvent::PlusOneCountersAdded {object: actual,count:3,..} if *actual == object)).count(), 1);
        }
        assert_eq!(restore(&state), state);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_doubled_haste_tokens_and_copied_nontoken_all_initialize_before_observers() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Battlefield),
        );
        let source = put(&mut state, actor, "Forest", Zone::Battlefield);
        let token = crate::card_def::card_id_by_name("Goblin Token").unwrap();
        let ctx = crate::effect::ExecCtx::no_targets(source, actor);
        let start = state.engine.event_history.len();
        crate::effect::execute(
            &EffectOp::CreateTokensWithHasteUntilEndOfTurnV1 {
                token_def: token,
                count: 2,
            },
            &ctx,
            &mut state,
        );
        let created = state.engine.event_history[start..]
            .iter()
            .filter_map(|event| match event {
                event::CommittedEvent::CreateToken { object, .. } => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(created.len(), 4);
        for object in created {
            assert!(crate::engine::has_effective_keyword(
                &state,
                object,
                Keywords::HASTE
            ));
        }
        let original = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let start = state.engine.event_history.len();
        crate::standard_cards_v1::create_token_copy(&mut state, original, actor).unwrap();
        let copies = state.engine.event_history[start..]
            .iter()
            .filter_map(|event| match event {
                event::CommittedEvent::CreateToken { object, .. } => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 2);
        for object in copies {
            assert!(state.objects.get(object).v4.is_token);
            assert_eq!(
                state.objects.get(object).card_def,
                state.objects.get(original).card_def
            );
        }
        assert_eq!(restore(&state), state);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_original_world_rejects_rewritten_replacement_roster_after_source_departure() {
    use crate::engine::{self, Action};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Battlefield),
        );
        let token = crate::card_def::card_id_by_name("Goblin Token").unwrap();
        event::propose_and_commit_batch(
            &mut state,
            vec![ProposedEvent::create_token(token, actor)],
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Graveyard),
        );
        validate_token_journals(&restore(&state)).unwrap();
        let original = state.engine.event_history.clone();
        for erase_input in [false, true] {
            let mut forged = restore(&state);
            if erase_input {
                let input = forged
                    .engine
                    .event_history
                    .iter_mut()
                    .find(|event| {
                        matches!(event, event::CommittedEvent::TokenCreationInputV48 { .. })
                    })
                    .unwrap();
                *input = event::CommittedEvent::Tap { object: season };
            } else {
                let creation = forged
                    .engine
                    .event_history
                    .iter_mut()
                    .find_map(|event| match event {
                        event::CommittedEvent::TokenCreationReplacementV48 { creation } => {
                            Some(creation)
                        }
                        _ => None,
                    })
                    .unwrap();
                creation.original[0].replacement.seasons.clear();
                creation.created.truncate(1);
            }
            for _ in 0..3 {
                let mut branch = restore(&forged);
                let before = bytes(&branch);
                assert!(engine::step(&mut branch, Action::Pass).is_err());
                assert_eq!(bytes(&branch), before);
            }
        }
        assert_eq!(state.engine.event_history, original);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn unrelated_entry_does_not_quote_unrequested_three_counters() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for _ in 0..30 {
            let source = put(&mut state, actor, "Doubling Season", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert!(state.engine.halted.is_none());
        }
        assert_eq!(sources(&state, actor).len(), 30);
        let forest = put(&mut state, actor, "Forest", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(forest, Zone::Battlefield),
        );
        assert!(state.engine.halted.is_none());
        assert_eq!(state.objects.get(forest).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(forest).counters, Default::default());
        assert_eq!(restore(&state), state);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_full_width_loyalty_and_entry_overflow_halt_before_any_move() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for _ in 0..29 {
            let source = put(&mut state, actor, "Doubling Season", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
        }
        let ajani = put(&mut state, actor, "Ajani, Caller of the Pride", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(ajani, Zone::Battlefield),
        );
        assert!(state.engine.halted.is_none());
        assert_eq!(
            crate::planeswalker_v1::loyalty(&state, ajani),
            Some(1_u32 << 31)
        );
        assert_eq!(restore(&state), state);
        let source = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        let overflowing = put(&mut state, actor, "Ajani, Caller of the Pride", Zone::Hand);
        let untouched = put(&mut state, actor, "Forest", Zone::Hand);
        let mut expected = restore(&state);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(untouched, Zone::Battlefield),
                ProposedEvent::zone_change(overflowing, Zone::Battlefield),
            ],
        );
        assert!(state.engine.halted.is_some());
        assert_eq!(state.objects.get(overflowing).zone, Zone::Hand);
        assert_eq!(state.objects.get(untouched).zone, Zone::Hand);
        expected.engine.halted = state.engine.halted;
        assert_eq!(state, expected);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn genuine_played_land_rule_entry_and_effect_entry_sample_distinct_origins() {
    use crate::engine::{self, Action};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Battlefield),
        );
        let land = put(&mut state, actor, "Forest", Zone::Hand);
        let mut rule = ProposedEvent::zone_change(land, Zone::Battlefield);
        if let ProposedEvent::ZoneChange(change) = &mut rule {
            change.counter_origin_v48 = Some(CounterOriginV48::Rule);
        }
        assert!(entry_input(&state, &rule).unwrap().seasons.is_empty());
        assert_eq!(
            entry_input(&state, &ProposedEvent::zone_change(land, Zone::Battlefield))
                .unwrap()
                .seasons
                .len(),
            1
        );
        engine::step(&mut state, Action::PlayLand(land)).unwrap();
        assert_eq!(state.objects.get(land).zone, Zone::Battlefield);
        assert_eq!(state.players[actor.index()].lands_played_this_turn, 1);
        assert_eq!(restore(&state), state);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn genuine_return_stun_primitive_uses_returned_owner_not_original_ability_controller() {
    use crate::effect::ExecCtx;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for same_owner in [true, false] {
            let mut state = ready_state_for(actor);
            let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(season, Zone::Battlefield),
            );
            let owner = if same_owner { actor } else { actor.opponent() };
            let source = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            if !same_owner {
                crate::standard_cards_v1::gain_control(&mut state, source, actor);
            }
            let contract = AbilitySourceContractV4::capture(&state, source);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            let mut ctx = ExecCtx::no_targets(source, actor);
            ctx.ability_source_contract = Some(contract);
            // Typed return primitive on a real physical incarnation. This
            // does not assign Llanowar Elves a fictional printed ability.
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                crate::effect::execute(
                    &EffectOp::ReturnSourceFromGraveyardTappedWithStunCounters { stun: 2 },
                    &ctx,
                    branch,
                );
                assert!(branch.engine.halted.is_none());
                assert_eq!(branch.objects.get(source).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(source).controller, owner);
                assert!(branch.objects.get(source).tapped);
                assert_eq!(
                    branch.objects.get(source).counters.stun,
                    if same_owner { 4 } else { 2 }
                );
            }
            assert_eq!(state, restored);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn frozen_visible_printing_refusal_uses_only_actor_current_knowledge_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let hidden = put(&mut state, actor.opponent(), "Doubling Season", Zone::Hand);
        put(&mut state, actor, "Forest", Zone::Hand);
        let before = bytes(&state);
        require_legacy_visible(&state, actor).unwrap();
        assert!(require_legacy_visible(&state, actor.opponent()).is_err());
        assert_eq!(bytes(&state), before);
        state
            .reveal_hand_card(actor, actor.opponent(), hidden)
            .unwrap();
        let before = bytes(&state);
        assert!(require_legacy_visible(&state, actor).is_err());
        assert_eq!(bytes(&state), before);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(hidden, Zone::Library),
        );
        // The real hand departure invalidates its old identity knowledge.
        require_legacy_visible(&state, actor).unwrap();
        state.reveal_library_top(actor, actor.opponent(), 1);
        let before = bytes(&state);
        assert!(require_legacy_visible(&state, actor).is_err());
        assert_eq!(bytes(&state), before);
        assert_eq!(restore(&state), state);
    }
}
