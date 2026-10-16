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
        let decision = engine::advance_until_decision(state);
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        if state.objects.get(source).zone == Zone::Battlefield {
            assert!(
                matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            assert!(state.engine.pending_cast.is_none());
            assert!(state.stack.is_empty());
            return source;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual Season spell did not resolve");
}

/// A genuine paid announcement supplies the Stack source's selected spell
/// form. Later entry/X operations are explicit primitives, not printed X spells
/// or a grant to cast another player's card.
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn paid_stack_source_for_entry_primitive(
    state: &mut GameState,
    owner: PlayerId,
    name: &str,
    target: Option<ObjectId>,
) -> ObjectId {
    use crate::engine::{self, Action, Decision};
    use crate::state::{CastMethodV4, StackItemKind, Target};
    assert_eq!(state.active_player, owner);
    assert_eq!(state.priority_player, owner);
    assert_eq!(state.players[owner.index()].mana_pool, [0; 6]);
    let source = put(state, owner, name, Zone::Hand);
    let def = &crate::card_def::CARD_DEFS[usize::from(state.objects.get(source).card_def)];
    assert_eq!(def.cost.x_count, 0);
    state.players[owner.index()].mana_pool[ManaColor::C.pool_index()] = def.cost.generic;
    for pip in &def.cost.pips {
        let crate::mana::Pip::Colored(color) = pip else {
            panic!("entry primitive fixture requires an ordinary colored printed cost");
        };
        state.players[owner.index()].mana_pool[color.pool_index()] += 1;
    }
    engine::step(state, Action::CastSpell(source)).unwrap();
    let mut decision = engine::advance_until_decision(state);
    if let Decision::ChooseTargets {
        player,
        spell: announced,
        ref legal_targets,
        ..
    } = decision
    {
        let target = target.expect("genuine announcement unexpectedly requires a target");
        assert_eq!(player, owner);
        assert_eq!(announced, source);
        assert!(legal_targets.contains(&Target::Object(target)));
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
        decision = engine::advance_until_decision(state);
    }
    assert!(matches!(decision, Decision::CastSpellOrPass { player, .. } if player == owner));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.players[owner.index()].mana_pool, [0; 6]);
    let live = state.objects.get(source);
    assert_eq!(live.owner, owner);
    assert_eq!(live.controller, owner);
    assert_eq!(live.zone, Zone::Stack);
    let origin = live.v4.spell_cast_origin.unwrap();
    assert_eq!(origin.route, crate::state::SpellCastRouteV4::Hand);
    assert_eq!(origin.finalized_method, Some(CastMethodV4::Normal));
    let row = state.stack.last().unwrap();
    assert_eq!(row.source, source);
    assert_eq!(row.kind, StackItemKind::Spell);
    assert_eq!(row.controller, owner);
    assert!(!row.is_copy);
    assert_eq!(row.v4.cast_method, Some(CastMethodV4::Normal));
    assert_eq!(
        row.targets,
        target.map(Target::Object).into_iter().collect::<Vec<_>>()
    );
    assert_eq!(row.v4.target_contracts.len(), usize::from(target.is_some()));
    assert!(crate::source_type_hexproof_v1::current_types(state, source).is_ok());
    source
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
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
                event::CommittedEvent::PlusOneCountersAdded {object: actual,count:3,..} if *actual == object)).count(),
                usize::from(cfg!(feature = "limited-fdn-fixtures")));
        }
        assert_eq!(restore(&state), state);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
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
        let input_index = state
            .engine
            .event_history
            .iter()
            .position(|event| matches!(event, event::CommittedEvent::TokenCreationInputV48 { .. }))
            .unwrap();
        let completion_index = state
            .engine
            .event_history
            .iter()
            .position(|event| {
                matches!(
                    event,
                    event::CommittedEvent::TokenCreationReplacementV48 { .. }
                )
            })
            .unwrap();
        let original_creations = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(index, event)| {
                matches!(event, event::CommittedEvent::CreateToken { .. })
                    .then_some((index, event.clone()))
            })
            .collect::<Vec<_>>();
        for forgery in 0..3 {
            let mut forged = restore(&state);
            if forgery == 2 {
                forged.engine.event_history[input_index] =
                    event::CommittedEvent::Tap { object: season };
                forged.engine.event_history[completion_index] =
                    event::CommittedEvent::Tap { object: season };
                for (index, original) in &original_creations {
                    assert_eq!(&forged.engine.event_history[*index], original);
                }
                assert!(
                    requires_successor(&forged),
                    "actual marked token retains the component after source departure"
                );
            } else if forgery == 1 {
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
                let mut automatic = restore(&forged);
                assert!(matches!(
                    engine::advance_until_decision(&mut automatic),
                    engine::Decision::Halted { .. }
                ));
                automatic.engine.halted = None;
                assert_eq!(bytes(&automatic), before);
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
                let observed: Vec<_> = branch
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|event| match event {
                        event::CommittedEvent::GarrukEntryObservationV1 { entry, .. }
                            if entry.object == source
                                && entry.snapshot.zone_change_count
                                    == branch.objects.get(source).zone_change_count =>
                        {
                            Some(entry)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(observed.len(), 1);
                assert_eq!(
                    observed[0].snapshot.counters.stun,
                    if same_owner { 4 } else { 2 }
                );
                assert!(observed[0].snapshot.tapped);
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

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_arahbo_self_entry_is_expanded_once_and_full_roster_closure_both_seats() {
    use crate::engine::{self, Action, Decision};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for owner in [actor, actor.opponent()] {
            put(&mut state, owner, "Forest", Zone::Battlefield);
            put(&mut state, owner, "Forest", Zone::Library);
        }
        cast_season(&mut state, actor);
        let source = put(&mut state, actor, "Arahbo, the First Fang", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::CastSpellOrPass {player,..} if player==actor)
        );
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        for _ in 0..32 {
            let decision = engine::advance_until_decision(&mut state);
            assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
            if state.objects.get(source).zone == Zone::Battlefield
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
            {
                break;
            }
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        let cat = crate::arahbo_v1::cat_token_definition().unwrap();
        let occurrences=state.engine.event_history.iter().enumerate().filter_map(|(i,event)|matches!(event,event::CommittedEvent::CreateToken {token_def,controller,..} if *token_def==cat && *controller==actor).then_some(i)).collect::<Vec<_>>();
        assert_eq!(occurrences.len(), 2);
        crate::arahbo_v1::validate_state(&state).unwrap();
        for _ in 0..3 {
            state = restore(&state);
            crate::arahbo_v1::validate_state(&state).unwrap();
        }
        let before = state.clone();
        // Retain original captured/publication/removal and completion. Changing
        // only the real second member must fail full volume authentication.
        state.engine.event_history[occurrences[1]] = event::CommittedEvent::Tap { object: source };
        let corrupted = state.clone();
        assert!(engine::step(&mut state, Action::Pass).is_err());
        assert_eq!(state, corrupted);
        assert_ne!(before, state);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_paid_self_reflection_arahbo_token_self_entry_and_legend_held_custody_both_seats() {
    use crate::engine::{self, Action, Decision};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for owner in [actor, actor.opponent()] {
            put(&mut state, owner, "Forest", Zone::Battlefield);
            put(&mut state, owner, "Forest", Zone::Library);
        }
        cast_season(&mut state, actor);
        let source = put(&mut state, actor, "Arahbo, the First Fang", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..32 {
            let decision = engine::advance_until_decision(&mut state);
            assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
            if state.objects.get(source).zone == Zone::Battlefield
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
            {
                break;
            }
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        let reflection = put(&mut state, actor, "Self-Reflection", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 6;
        engine::step(&mut state, Action::CastSpell(reflection)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::ChooseTargets {spell,remaining:1,can_finish:false,..} if spell==reflection)
        );
        engine::step(
            &mut state,
            Action::ChooseTarget(crate::state::Target::Object(source)),
        )
        .unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::CastSpellOrPass {player,..} if player==actor)
        );
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        let start = state.engine.event_history.len();
        let mut reached = false;
        for _ in 0..32 {
            let decision = engine::advance_until_decision(&mut state);
            if let Decision::ChooseLegendPermanent { player, candidates } = &decision {
                assert_eq!(*player, actor);
                assert_eq!(candidates.len(), 3);
                assert!(candidates.contains(&source));
                reached = true;
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(reached);
        let copies = state.engine.event_history[start..]
            .iter()
            .filter_map(|event| match event {
                event::CommittedEvent::CreateToken {
                    object,
                    token_def,
                    controller,
                    ..
                } if *token_def == state.objects.get(source).card_def && *controller == actor => {
                    Some(*object)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().all(|id| state.objects.get(*id).v4.is_token));
        let captures = state.engine.event_history[start..]
            .iter()
            .filter_map(|event| match event {
                event::CommittedEvent::ArahboTokenCaptureV1 { capture } => Some(capture),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(captures.len(), 2);
        assert!(captures
            .iter()
            .all(|capture| capture.root == crate::arahbo_v1::RootV1::OwnEntry
                && copies.contains(&capture.source.source)));
        crate::arahbo_v1::validate_state(&state).unwrap();
        let cat = crate::arahbo_v1::cat_token_definition().unwrap();
        let original = state.clone();
        for keep in std::iter::once(source).chain(copies.iter().copied()) {
            let mut branch = restore(&original);
            engine::step(&mut branch, Action::ChooseLegendPermanent(keep)).unwrap();
            for _ in 0..48 {
                let decision = engine::advance_until_decision(&mut branch);
                assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
                if branch.stack.is_empty()
                    && branch.engine.pending_triggers.is_empty()
                    && branch.pending_legend_rule_v1.is_none()
                {
                    break;
                }
                match decision {
                    Decision::OrderTriggers { pending, .. } => engine::step(
                        &mut branch,
                        Action::OrderTriggers((0..pending.len()).collect()),
                    )
                    .unwrap(),
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut branch, Action::Pass).unwrap()
                    }
                    other => panic!("unexpected genuine copy continuation {other:?}"),
                }
            }
            assert_eq!(branch.engine.event_history[start..].iter().filter(|event|matches!(event,event::CommittedEvent::CreateToken {token_def,controller,..} if *token_def==cat && *controller==actor)).count(),4);
            assert_eq!(branch.objects.get(keep).zone, Zone::Battlefield);
            crate::arahbo_v1::validate_state(&branch).unwrap();
            crate::doubling_season_v48::validate_token_journals(&branch).unwrap();
            assert_eq!(restore(&branch), branch);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_stun_return_overflow_leaves_original_graveyard_incarnation_unchanged() {
    use crate::effect::ExecCtx;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Battlefield),
        );
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let contract = AbilitySourceContractV4::capture(&state, source);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        let mut ctx = ExecCtx::no_targets(source, actor);
        ctx.ability_source_contract = Some(contract);
        let original = restore(&state);
        for _ in 0..3 {
            let mut branch = restore(&original);
            crate::effect::execute(
                &EffectOp::ReturnSourceFromGraveyardTappedWithStunCounters { stun: i16::MAX },
                &ctx,
                &mut branch,
            );
            assert_eq!(
                branch.engine.halted,
                Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    source
                ))
            );
            branch.engine.halted = None;
            assert_eq!(bytes(&branch), bytes(&original));
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn shared_x_entry_primitives_initialize_before_observers_with_full_native_count_and_host() {
    use crate::effect::{ExecCtx, ObjectRef};
    use crate::state::{StackTargetContractV4, Target};
    for actor in [PlayerId::P0, PlayerId::P1] {
        for attached in [false, true] {
            for original in [0, 2, u16::MAX] {
                let mut state = ready_state_for(actor);
                let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(season, Zone::Battlefield),
                );
                let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                let source = paid_stack_source_for_entry_primitive(
                    &mut state,
                    actor,
                    if attached {
                        "Pacifism"
                    } else {
                        "Llanowar Elves"
                    },
                    attached.then_some(host),
                );
                let mut ctx = ExecCtx::no_targets(source, actor);
                ctx.x_value = original;
                let op = if attached {
                    ctx.targets = vec![Target::Object(host)];
                    ctx.target_contracts =
                        vec![StackTargetContractV4::capture(&state, Target::Object(host))];
                    EffectOp::PutSourceOntoBattlefieldAttachedToTargetWithXPlusOneCounters {
                        target: ObjectRef::Target(0),
                    }
                } else {
                    EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters
                };
                // Paid ordinary spells supply real source-form authority. X is
                // assigned only to this shared typed primitive; neither printing
                // gains a fictional printed X cost or effect.
                let original_state = restore(&state);
                for _ in 0..3 {
                    let mut branch = restore(&original_state);
                    crate::effect::execute(&op, &ctx, &mut branch);
                    assert!(branch.engine.halted.is_none());
                    let actual = i32::from(original) * 2;
                    let live = branch.objects.get(source);
                    assert_eq!(live.zone, Zone::Battlefield);
                    assert_eq!(live.counters.plus1_plus1, actual);
                    if attached {
                        assert_eq!(
                            live.v4.attached_to,
                            Some(crate::state::ObjectLinkV4 {
                                object: host,
                                zone_change_count: branch.objects.get(host).zone_change_count
                            })
                        );
                        assert!(branch.objects.get(host).attachments.contains(&source));
                        branch.validate_attachment_relations().unwrap();
                    }
                    let observed: Vec<_> =
                        branch
                            .engine
                            .event_history
                            .iter()
                            .filter_map(|event| match event {
                                event::CommittedEvent::GarrukEntryObservationV1 {
                                    entry, ..
                                } if entry.object == source => Some(entry),
                                _ => None,
                            })
                            .collect();
                    assert_eq!(observed.len(), 1);
                    assert_eq!(observed[0].snapshot.counters.plus1_plus1, actual);
                    assert_eq!(observed[0].snapshot.v4.attached_to, live.v4.attached_to);
                    assert_eq!(branch.engine.event_history.iter().filter(|event| matches!(event, event::CommittedEvent::PlusOneCountersAdded { object, count, .. } if *object == source && *count == actual)).count(), usize::from(original != 0));
                    validate_token_journals(&branch).unwrap();
                    assert_eq!(restore(&branch), branch);
                }
            }
        }
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn assert_genuine_permanent_spell_copy_entry(
    actor: PlayerId,
    copy_controller: PlayerId,
    season_mode: Option<bool>,
) {
    use crate::engine::{self, Action, Decision};
    let mut state = ready_state_for(actor);
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Forest", Zone::Hand);
        put(&mut state, player, "Forest", Zone::Library);
    }
    if let Some(departed) = season_mode {
        let season = cast_season(&mut state, actor);
        if departed {
            let goblin = crate::card_def::card_id_by_name("Goblin Token").unwrap();
            event::propose_and_commit_batch(
                &mut state,
                vec![ProposedEvent::create_token(goblin, actor)],
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(season, Zone::Graveyard),
            );
            assert!(sources(&state, actor).is_empty());
            assert!(requires_successor(&state));
            validate_token_journals(&state).unwrap();
        }
    } else {
        assert!(!requires_successor(&state));
    }
    let source = put(&mut state, actor, "Arahbo, the First Fang", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 3;
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    let (copy, copy_stack) = engine::copy_spell_for(&mut state, source, copy_controller).unwrap();
    assert!(
        state.engine.halted.is_none(),
        "permanent-copy publication halted"
    );
    assert_ne!(copy, source);
    assert_eq!(state.objects.get(copy).zone, Zone::Stack);
    assert!(
        !state.objects.get(copy).v4.is_token,
        "a spell copy is not yet a token"
    );
    assert!(!state.objects.get(source).v4.is_token);
    assert!(state
        .stack
        .iter()
        .any(|row| row.source == copy && row.v4.stack_item_id == copy_stack && row.is_copy));
    let start = state.engine.event_history.len();
    let original = restore(&state);
    for _ in 0..3 {
        let mut branch = restore(&original);
        for _ in 0..32 {
            if let Some(item) = branch.stack.last() {
                let validation = engine::validated_stack_item_target_spec(item, &branch);
                assert!(
                    validation.is_ok(),
                    "permanent-copy pre-advance stack validation rejected: {validation:?}"
                );
            }
            let decision = engine::advance_until_decision(&mut branch);
            assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
            if branch.objects.get(copy).zone == Zone::Battlefield {
                break;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => {
                    engine::step(&mut branch, Action::Pass).unwrap()
                }
                Decision::OrderTriggers { pending, .. } => engine::step(
                    &mut branch,
                    Action::OrderTriggers((0..pending.len()).collect()),
                )
                .unwrap(),
                other => panic!("unexpected permanent copy entry {other:?}"),
            }
        }
        let live = branch.objects.get(copy);
        assert_eq!(live.zone, Zone::Battlefield);
        assert_eq!(live.controller, copy_controller);
        assert!(live.v4.is_token);
        assert_eq!(branch.objects.get(source).zone, Zone::Stack);
        assert_eq!(branch.engine.event_history[start..].iter().filter(|event| matches!(event, event::CommittedEvent::CreateToken { token_def, .. } if *token_def == live.card_def)).count(), 0);
        let entries: Vec<_> = branch.engine.event_history[start..]
            .iter()
            .filter_map(|event| match event {
                event::CommittedEvent::GarrukEntryObservationV1 { entry, .. }
                    if entry.object == copy =>
                {
                    Some(entry)
                }
                _ => None,
            })
            .collect();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].snapshot.v4.is_token);
        assert_eq!(entries[0].snapshot.controller, copy_controller);
        let captures: Vec<_> = branch.engine.event_history[start..]
            .iter()
            .filter_map(|event| match event {
                event::CommittedEvent::ArahboTokenCaptureV1 { capture }
                    if capture.source.source == copy =>
                {
                    Some(capture)
                }
                _ => None,
            })
            .collect();
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].root, crate::arahbo_v1::RootV1::OwnEntry);
        assert_eq!(captures[0].source.controller, copy_controller);
        crate::arahbo_v1::validate_state(&branch).unwrap();
        validate_token_journals(&branch).unwrap();
        assert_eq!(restore(&branch), branch);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_no_season_paid_permanent_copy_is_nontoken_on_stack_and_token_at_entry() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for copy_controller in [actor, actor.opponent()] {
            assert_genuine_permanent_spell_copy_entry(actor, copy_controller, None);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_paid_permanent_spell_copy_becomes_one_token_not_a_doubled_creation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for copy_controller in [actor, actor.opponent()] {
            for departed in [false, true] {
                assert_genuine_permanent_spell_copy_entry(actor, copy_controller, Some(departed));
            }
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn nonowned_stack_entry_samples_actual_spell_controller_not_owner() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for owner_controls_season in [false, true] {
            let owner = actor.opponent();
            let mut state = ready_state_for(owner);
            let season_controller = if owner_controls_season { owner } else { actor };
            let season = put(&mut state, season_controller, "Doubling Season", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(season, Zone::Battlefield),
            );
            let source =
                paid_stack_source_for_entry_primitive(&mut state, owner, "Mossborn Hydra", None);
            // The physical owner genuinely paid its ordinary cast. The following
            // Stack-controller assignment remains an explicit primitive input;
            // it claims neither a printed control-changing spell nor permission
            // for actor to cast the foreign card.
            state.objects.get_mut(source).controller = actor;
            let proposal = ProposedEvent::zone_change(source, Zone::Battlefield);
            let before = bytes(&state);
            let input = entry_input(&state, &proposal).unwrap();
            assert_eq!(input.controller, actor);
            assert_eq!(input.seasons.len(), usize::from(!owner_controls_season));
            assert_eq!(bytes(&state), before);
            for _ in 0..3 {
                let mut branch = restore(&state);
                event::propose_and_commit(&mut branch, proposal.clone());
                assert!(branch.engine.halted.is_none());
                let live = branch.objects.get(source);
                assert_eq!(live.owner, owner);
                assert_eq!(live.controller, actor);
                assert_eq!(live.zone, Zone::Battlefield);
                assert_eq!(
                    live.counters.plus1_plus1,
                    if owner_controls_season { 1 } else { 2 }
                );
                let entry = branch
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        event::CommittedEvent::GarrukEntryObservationV1 { entry, .. }
                            if entry.object == source =>
                        {
                            Some(entry)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(entry.snapshot.owner, owner);
                assert_eq!(entry.snapshot.controller, actor);
                assert_eq!(
                    entry.snapshot.counters.plus1_plus1,
                    live.counters.plus1_plus1
                );
                assert_eq!(restore(&branch), branch);
            }
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn native_loyalty_effect_and_cost_quotes_remain_distinct_at_signed_boundary() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Battlefield),
        );
        let recipient = put(&mut state, actor, "Forest", Zone::Battlefield);
        // Explicit native counter primitive, not a fictional Forest loyalty ability.
        crate::planeswalker_v1::add_loyalty_counters_u32_v48(&mut state, recipient, 1).unwrap();
        assert_eq!(crate::planeswalker_v1::loyalty(&state, recipient), Some(2));
        let original = restore(&state);
        for _ in 0..3 {
            let mut cost = restore(&original);
            crate::planeswalker_v1::pay_loyalty_cost(&mut cost, recipient, i32::MAX);
            assert!(cost.engine.halted.is_none());
            assert_eq!(
                crate::planeswalker_v1::loyalty(&cost, recipient),
                Some(i32::MAX as u32 + 2)
            );
            let mut effect = restore(&original);
            crate::planeswalker_v1::change_loyalty_effect(&mut effect, recipient, i32::MAX);
            assert!(effect.engine.halted.is_some());
            effect.engine.halted = None;
            assert_eq!(bytes(&effect), bytes(&original));
            let mut added = restore(&original);
            crate::planeswalker_v1::add_loyalty_counters(&mut added, recipient, i32::MAX);
            assert!(added.engine.halted.is_some());
            added.engine.halted = None;
            assert_eq!(bytes(&added), bytes(&original));
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn original_effect_counter_world_binds_actor_recipient_and_actual_placement_before_actions() {
    use crate::engine::{self, Action};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Forest", Zone::Hand);
            put(&mut state, player, "Forest", Zone::Library);
        }
        cast_season(&mut state, actor);
        let recipient = put(&mut state, actor, "Forest", Zone::Battlefield);
        event::add_plus_one_counters(&mut state, recipient, actor.opponent(), 3).unwrap();
        assert_eq!(state.objects.get(recipient).counters.plus1_plus1, 6);
        validate_token_journals(&state).unwrap();
        let placement_index = state
            .engine
            .event_history
            .iter()
            .position(|event| {
                matches!(
                    event,
                    event::CommittedEvent::PlusCounterReplacementV48 { .. }
                )
            })
            .unwrap();
        let event::CommittedEvent::PlusCounterReplacementV48 { placement } =
            &state.engine.event_history[placement_index]
        else {
            unreachable!()
        };
        let original_placement = placement.clone();
        let input_index = original_placement.input as usize;
        let counter_index = original_placement.counter_event as usize;
        assert!(input_index < counter_index && counter_index < placement_index);
        assert_eq!(
            expected_plus_at(&state, counter_index as u64, recipient, actor.opponent(), 3).unwrap(),
            6
        );
        assert!(expected_plus_at(&state, counter_index as u64, recipient, actor, 3).is_err());
        assert!(
            expected_plus_at(&state, counter_index as u64, recipient, actor.opponent(), 4).is_err()
        );
        for _ in 0..3 {
            let valid = restore(&state);
            validate_token_journals(&valid).unwrap();
            assert_eq!(bytes(&valid), bytes(&state));
            for forgery in 0..6 {
                let mut forged = restore(&state);
                match forgery {
                    0 => {
                        if let event::CommittedEvent::PlusCounterReplacementV48 { placement } =
                            &mut forged.engine.event_history[placement_index]
                        {
                            placement.original = 4;
                            placement.after = 8;
                        }
                        if let event::CommittedEvent::PlusOneCountersAdded { count, .. } =
                            &mut forged.engine.event_history[counter_index]
                        {
                            *count = 8;
                        }
                        forged.objects.get_mut(recipient).counters.plus1_plus1 = 8;
                    }
                    1 => {
                        if let event::CommittedEvent::PlusCounterReplacementV48 { placement } =
                            &mut forged.engine.event_history[placement_index]
                        {
                            placement.actor = actor;
                        }
                        if let event::CommittedEvent::PlusOneCountersAdded { player, .. } =
                            &mut forged.engine.event_history[counter_index]
                        {
                            *player = actor;
                        }
                    }
                    2 => {
                        forged.engine.event_history[input_index] =
                            event::CommittedEvent::Tap { object: recipient }
                    }
                    3 => {
                        forged.engine.event_history[placement_index] =
                            event::CommittedEvent::Tap { object: recipient }
                    }
                    4 => forged
                        .engine
                        .event_log
                        .push(forged.engine.event_history[input_index].clone()),
                    5 => {
                        forged.engine.event_history[input_index] =
                            event::CommittedEvent::Tap { object: recipient };
                        forged.engine.event_history[placement_index] =
                            event::CommittedEvent::Tap { object: recipient };
                        // Preserve the actual issued six-counter occurrence,
                        // including its intrinsic original-input reference.
                        assert_eq!(
                            forged.engine.event_history[counter_index],
                            state.engine.event_history[counter_index]
                        );
                        assert_eq!(forged.objects.get(recipient).counters.plus1_plus1, 6);
                        assert!(expected_plus_at(
                            &forged,
                            counter_index as u64,
                            recipient,
                            actor.opponent(),
                            3
                        )
                        .is_err());
                    }
                    _ => unreachable!(),
                }
                let before = bytes(&forged);
                if forgery < 3 {
                    assert!(expected_plus_at(
                        &forged,
                        counter_index as u64,
                        recipient,
                        actor.opponent(),
                        3
                    )
                    .is_err());
                }
                assert!(validate_token_journals(&forged).is_err());
                assert!(engine::step(&mut forged, Action::Pass).is_err());
                assert_eq!(bytes(&forged), before);
                let mut automatic = restore(&forged);
                assert!(matches!(
                    engine::advance_until_decision(&mut automatic),
                    engine::Decision::Halted { .. }
                ));
                automatic.engine.halted = None;
                assert_eq!(bytes(&automatic), before);
            }
        }
        let before = bytes(&state);
        assert!(event::add_plus_one_counters(&mut state, recipient, actor, i32::MAX).is_err());
        assert_eq!(bytes(&state), before);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn retained_replacement_history_keeps_single_token_initialization_before_entry() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Battlefield),
        );
        let token_def = crate::card_def::card_id_by_name("Goblin Token").unwrap();
        event::propose_and_commit_batch(
            &mut state,
            vec![ProposedEvent::create_token(token_def, actor)],
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(season, Zone::Graveyard),
        );
        assert!(sources(&state, actor).is_empty());
        assert!(requires_successor(&state));
        validate_token_journals(&state).unwrap();
        let source = put(&mut state, actor, "Forest", Zone::Battlefield);
        let donor = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let original = restore(&state);
        let mut result = None;
        for _ in 0..3 {
            let mut branch = restore(&original);
            let ctx = crate::effect::ExecCtx::no_targets(source, actor);
            let start = branch.engine.event_history.len();
            crate::effect::execute(
                &EffectOp::CreateTokensDynamic {
                    token_def,
                    controller: crate::effect::PlayerRef::Controller,
                    count: crate::card_def::DynamicValueDef::Fixed(2),
                    tapped: true,
                },
                &ctx,
                &mut branch,
            );
            let created = branch.engine.event_history[start..]
                .iter()
                .filter_map(|event| match event {
                    event::CommittedEvent::CreateToken {
                        object,
                        token_def: actual,
                        controller,
                        ..
                    } if *actual == token_def && *controller == actor => Some(*object),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                created.len(),
                2,
                "departed Season must not multiply new tokens"
            );
            for object in created {
                let observed = branch.engine.event_history[start..]
                    .iter()
                    .filter_map(|event| match event {
                        event::CommittedEvent::GarrukEntryObservationV1 { entry, .. }
                            if entry.object == object =>
                        {
                            Some(entry)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(observed.len(), 1);
                assert!(
                    observed[0].snapshot.tapped,
                    "tapped is an entry characteristic"
                );
                assert!(observed[0].snapshot.v4.is_token);
            }
            let start = branch.engine.event_history.len();
            crate::effect::execute(
                &EffectOp::CreateTokensWithHasteUntilEndOfTurnV1 {
                    token_def,
                    count: 2,
                },
                &ctx,
                &mut branch,
            );
            let hasty = branch.engine.event_history[start..]
                .iter()
                .filter_map(|event| match event {
                    event::CommittedEvent::CreateToken { object, .. } => Some(*object),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(hasty.len(), 2);
            for object in hasty {
                assert!(crate::engine::has_effective_keyword(
                    &branch,
                    object,
                    Keywords::HASTE
                ));
            }
            let start = branch.engine.event_history.len();
            let copy =
                crate::standard_cards_v1::create_token_copy(&mut branch, donor, actor).unwrap();
            assert_eq!(
                branch.engine.event_history[start..]
                    .iter()
                    .filter(|event| matches!(event, event::CommittedEvent::CreateToken { .. }))
                    .count(),
                1
            );
            let snapshot = branch.engine.event_history[start..]
                .iter()
                .find_map(|event| match event {
                    event::CommittedEvent::GarrukEntryObservationV1 { entry, .. }
                        if entry.object == copy =>
                    {
                        Some(&entry.snapshot)
                    }
                    _ => None,
                })
                .unwrap();
            assert!(snapshot.v4.is_token);
            assert_eq!(snapshot.card_def, branch.objects.get(donor).card_def);
            assert!(branch.engine.halted.is_none());
            validate_token_journals(&branch).unwrap();
            assert_eq!(restore(&branch), branch);
            if let Some(expected) = &result {
                assert_eq!(&branch, expected);
            }
            result = Some(branch);
        }
    }
}

#[test]
#[cfg(not(any(
    feature = "limited-fdn-fixtures",
    feature = "standard-magezero-fixtures"
)))]
fn genuine_wall_of_roots_cost_is_undoubled_and_overflow_is_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let roots = put(&mut state, actor, "Wall of Roots", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(roots, Zone::Battlefield),
        );
        assert!(!requires_successor(&state));
        let before = state.players[actor.index()].mana_pool[ManaColor::G.pool_index()];
        let original = restore(&state);
        let mut result = None;
        for _ in 0..3 {
            let mut branch = restore(&original);
            crate::engine::step(
                &mut branch,
                crate::engine::Action::ActivateManaAbility(roots),
            )
            .unwrap();
            assert_eq!(branch.objects.get(roots).counters.minus0_minus1, 1);
            assert_eq!(
                branch.players[actor.index()].mana_pool[ManaColor::G.pool_index()],
                before + 1
            );
            assert!(!branch.objects.get(roots).tapped);
            assert!(!requires_successor(&branch));
            if let Some(expected) = &result {
                assert_eq!(&branch, expected);
            }
            result = Some(branch);
        }
        // Native storage boundary for the actual printed counter cost, not
        // a synthetic counter ability or grant grafted on another printing.
        state.objects.get_mut(roots).counters.minus0_minus1 = i16::MAX;
        for _ in 0..3 {
            let mut branch = restore(&state);
            let before = bytes(&branch);
            assert!(crate::engine::step(
                &mut branch,
                crate::engine::Action::ActivateManaAbility(roots)
            )
            .is_err());
            assert_eq!(bytes(&branch), before);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn ordinary_real_counter_occurrence_keeps_legacy_bytes_and_derived_hash() {
    use std::hash::{Hash, Hasher};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let recipient = put(&mut state, actor, "Forest", Zone::Battlefield);
        event::add_plus_one_counters(&mut state, recipient, actor, 3).unwrap();
        let occurrence = state.engine.event_history.iter().find(|event|
            matches!(event, event::CommittedEvent::PlusOneCountersAdded { object, .. } if *object == recipient)).unwrap();
        let event::CommittedEvent::PlusOneCountersAdded {
            object,
            zone_change_count,
            player,
            count,
            replacement_input_v48,
        } = occurrence
        else {
            unreachable!()
        };
        assert!(replacement_input_v48.is_none());
        let mut predecessor = std::collections::hash_map::DefaultHasher::new();
        std::mem::discriminant(occurrence).hash(&mut predecessor);
        object.hash(&mut predecessor);
        zone_change_count.hash(&mut predecessor);
        player.hash(&mut predecessor);
        count.hash(&mut predecessor);
        let mut current = std::collections::hash_map::DefaultHasher::new();
        occurrence.hash(&mut current);
        assert_eq!(predecessor.finish(), current.finish());
        let value = serde_json::to_value(occurrence).unwrap();
        assert!(value["PlusOneCountersAdded"]
            .get("replacement_input_v48")
            .is_none());
        assert_eq!(value["PlusOneCountersAdded"].as_object().unwrap().len(), 4);
        let legacy: event::CommittedEvent = serde_json::from_value(value).unwrap();
        assert_eq!(&legacy, occurrence);
        assert_eq!(restore(&state), state);
    }
}

#[test]
fn ordinary_actual_token_occurrence_keeps_legacy_bytes_and_derived_hash() {
    use std::hash::{Hash, Hasher};
    let token_def = CARD_DEFS
        .iter()
        .position(|d| d.is_token && d.has_full_support())
        .unwrap() as u16;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        event::propose_and_commit_batch(
            &mut state,
            vec![ProposedEvent::create_token(token_def, actor)],
        );
        let occurrence = state
            .engine
            .event_history
            .iter()
            .find(|event| matches!(event, event::CommittedEvent::CreateToken { .. }))
            .unwrap();
        let event::CommittedEvent::CreateToken {
            object,
            token_def,
            controller,
            replacement_input_v48,
        } = occurrence
        else {
            unreachable!()
        };
        assert!(replacement_input_v48.is_none());
        let mut predecessor = std::collections::hash_map::DefaultHasher::new();
        std::mem::discriminant(occurrence).hash(&mut predecessor);
        object.hash(&mut predecessor);
        token_def.hash(&mut predecessor);
        controller.hash(&mut predecessor);
        let mut current = std::collections::hash_map::DefaultHasher::new();
        occurrence.hash(&mut current);
        assert_eq!(predecessor.finish(), current.finish());
        let value = serde_json::to_value(occurrence).unwrap();
        assert!(value["CreateToken"].get("replacement_input_v48").is_none());
        assert_eq!(value["CreateToken"].as_object().unwrap().len(), 3);
        let legacy: event::CommittedEvent = serde_json::from_value(value).unwrap();
        assert_eq!(&legacy, occurrence);
        assert_eq!(restore(&state), state);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_full_ajani_zero_target_cost_uses_same_quote_as_payment_before_resolution() {
    use crate::engine::{self, Action, Decision};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Forest", Zone::Hand);
            put(&mut state, player, "Forest", Zone::Library);
        }
        cast_season(&mut state, actor);
        put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let ajani = put(&mut state, actor, "Ajani, Caller of the Pride", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 3;
        engine::step(&mut state, Action::CastSpell(ajani)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        for _ in 0..24 {
            let decision = engine::advance_until_decision(&mut state);
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            if state.objects.get(ajani).zone == Zone::Battlefield && state.stack.is_empty() {
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { player, ref activatable_abilities, .. }
                    if player == actor && activatable_abilities.contains(&(ajani, 0)))
                );
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(crate::planeswalker_v1::loyalty(&state, ajani), Some(8));
        let cost = crate::planeswalker_v1::quote_loyalty_change_v48(
            &state,
            ajani,
            1,
            CounterOriginV48::Cost,
        )
        .unwrap();
        let effect = crate::planeswalker_v1::quote_loyalty_change_v48(
            &state,
            ajani,
            1,
            CounterOriginV48::Effect,
        )
        .unwrap();
        assert_eq!(cost, 9, "unmodified printed +1 cost is not doubled");
        assert_eq!(effect, 10, "effect counter placement is doubled");
        let original = restore(&state);
        let mut result = None;
        for _ in 0..3 {
            let mut branch = restore(&original);
            engine::step(&mut branch, Action::ActivateAbility(ajani, 0)).unwrap();
            assert!(
                matches!(engine::advance_until_decision(&mut branch), Decision::ChooseEffectTargets { source, can_finish: true, min_targets: 0, max_targets: 1, .. } if source == ajani)
            );
            engine::step(&mut branch, Action::FinishEffectSelection).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut branch),
                Decision::CastSpellOrPass { .. }
            ));
            assert_eq!(crate::planeswalker_v1::loyalty(&branch, ajani), Some(cost));
            assert!(branch
                .stack
                .iter()
                .any(|item| item.source == ajani && item.targets.is_empty()));
            crate::ajani_full_v42::lifecycle::validate_state(&branch).unwrap();
            for _ in 0..24 {
                if branch.stack.is_empty() {
                    break;
                }
                assert!(matches!(
                    engine::advance_until_decision(&mut branch),
                    Decision::CastSpellOrPass { .. }
                ));
                engine::step(&mut branch, Action::Pass).unwrap();
            }
            assert!(branch.stack.is_empty());
            assert_eq!(crate::planeswalker_v1::loyalty(&branch, ajani), Some(cost));
            crate::ajani_full_v42::lifecycle::validate_state(&branch).unwrap();
            assert_eq!(restore(&branch), branch);
            if let Some(expected) = &result {
                assert_eq!(&branch, expected);
            }
            result = Some(branch);
        }
    }
}

#[test]
#[cfg(feature = "standard-magezero-fixtures")]
fn genuine_talent_level_order_changes_cost_rule_and_effect_quotes_without_joint_catalog() {
    use crate::engine::{self, Action, Decision};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Forest", Zone::Hand);
            put(&mut state, player, "Forest", Zone::Library);
        }
        let recipient = put(&mut state, actor.opponent(), "Forest", Zone::Battlefield);
        let talent = put(&mut state, actor, "Innkeeper's Talent", Zone::Hand);
        assert!(
            !requires_successor(&state),
            "this is the real Standard catalog without Season"
        );
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(talent)).unwrap();
        engine::advance_until_decision(&mut state);
        for _ in 0..24 {
            if state.objects.get(talent).zone == Zone::Battlefield && state.stack.is_empty() {
                break;
            }
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(crate::standard_cards_v1::class_level(&state, talent), 1);
        for origin in [
            CounterOriginV48::Cost,
            CounterOriginV48::Rule,
            CounterOriginV48::Effect,
        ] {
            assert_eq!(
                counter_amount(&state, recipient, actor, origin, 1).unwrap(),
                1
            );
        }
        for (ability, mana, level) in [(0, 1, 2), (1, 4, 3)] {
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = mana;
            engine::step(&mut state, Action::ActivateAbility(talent, ability)).unwrap();
            engine::advance_until_decision(&mut state);
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            for _ in 0..24 {
                if state.stack.is_empty() {
                    break;
                }
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::CastSpellOrPass { .. }
                ));
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert_eq!(crate::standard_cards_v1::class_level(&state, talent), level);
        }
        for _ in 0..3 {
            let mut branch = restore(&state);
            for origin in [
                CounterOriginV48::Cost,
                CounterOriginV48::Rule,
                CounterOriginV48::Effect,
            ] {
                assert_eq!(
                    counter_amount(&branch, recipient, actor, origin, 1).unwrap(),
                    2
                );
                assert_eq!(
                    counter_amount(&branch, recipient, actor.opponent(), origin, 1).unwrap(),
                    1
                );
            }
            event::propose_and_commit(&mut branch, ProposedEvent::zone_change(talent, Zone::Hand));
            for origin in [
                CounterOriginV48::Cost,
                CounterOriginV48::Rule,
                CounterOriginV48::Effect,
            ] {
                assert_eq!(
                    counter_amount(&branch, recipient, actor, origin, 1).unwrap(),
                    1
                );
            }
            assert!(!requires_successor(&branch));
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_offer_counter_reward_is_one_whole_expanded_entry_roster_for_original_controller() {
    use crate::engine::{self, Action, Decision};
    use crate::state::Target;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Forest", Zone::Battlefield);
            put(&mut state, player, "Forest", Zone::Hand);
            put(&mut state, player, "Forest", Zone::Library);
        }
        cast_season(&mut state, actor);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { spell, .. } if spell == bolt)
        );
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
        );
        let offer = put(
            &mut state,
            actor.opponent(),
            "An Offer You Can't Refuse",
            Zone::Hand,
        );
        state.players[actor.opponent().index()].mana_pool[ManaColor::U.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(offer)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets == vec![Target::Object(bolt)])
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(bolt))).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
        );
        assert_eq!(state.players[actor.opponent().index()].mana_pool, [0; 6]);
        let original = restore(&state);
        let start = state.engine.event_history.len();
        let treasure = crate::card_def::card_id_by_name("Treasure Token").unwrap();
        let mut result = None;
        for _ in 0..3 {
            let mut branch = restore(&original);
            for _ in 0..32 {
                if branch.stack.is_empty() {
                    break;
                }
                assert!(matches!(
                    engine::advance_until_decision(&mut branch),
                    Decision::CastSpellOrPass { .. }
                ));
                engine::step(&mut branch, Action::Pass).unwrap();
            }
            assert!(branch.stack.is_empty());
            assert_eq!(branch.objects.get(bolt).zone, Zone::Graveyard);
            assert_eq!(branch.objects.get(offer).zone, Zone::Graveyard);
            assert_eq!(
                branch.players[actor.opponent().index()].life,
                20,
                "actual Bolt was countered"
            );
            let actual = branch.engine.event_history[start..]
                .iter()
                .filter_map(|event| match event {
                    event::CommittedEvent::CreateToken {
                        object,
                        token_def,
                        controller,
                        replacement_input_v48,
                    } if *token_def == treasure => {
                        assert_eq!(
                            *controller, actor,
                            "reward belongs to countered spell's original controller"
                        );
                        Some((*object, replacement_input_v48.0.unwrap()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(actual.len(), 4);
            assert!(actual.iter().all(|(_, input)| *input == actual[0].1));
            let completions = branch.engine.event_history[start..]
                .iter()
                .filter_map(|event| match event {
                    event::CommittedEvent::TokenCreationReplacementV48 { creation }
                        if creation.input == actual[0].1 =>
                    {
                        Some(creation)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(completions.len(), 1);
            assert_eq!(completions[0].original.len(), 2);
            assert_eq!(completions[0].created.len(), 4);
            validate_token_journals(&branch).unwrap();
            assert_eq!(restore(&branch), branch);
            if let Some(expected) = &result {
                assert_eq!(&branch, expected);
            }
            result = Some(branch);
        }
    }
}

#[test]
#[cfg(feature = "standard-magezero-fixtures")]
fn paid_level_three_talent_checked_counter_placement_keeps_unsigned_loyalty_and_overflow_both_seats(
) {
    use crate::engine::{self, Action, Decision};
    use crate::standard_creatures_v1::{CounterExtrasV1, CounterTransferV1};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for _ in 0..12 {
            put(&mut state, actor, "Forest", Zone::Battlefield);
        }
        let recipient = put(&mut state, actor, "Forest", Zone::Battlefield);
        let overflow = put(&mut state, actor, "Forest", Zone::Battlefield);
        // Explicit native counter-placement operands. The genuine Standard
        // catalog supplies Talent, but does not register a fake Zimone joint
        // printing. This test exercises the shared placement primitive only.
        for (target, loyalty) in [(recipient, 1_100_000_000), (overflow, 1_500_000_000)] {
            crate::standard_creatures_v1::place_counters_checked_v1(
                &mut state,
                target,
                actor,
                CounterTransferV1 {
                    extras: CounterExtrasV1 {
                        loyalty,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let talent = put(&mut state, actor, "Innkeeper's Talent", Zone::Hand);
        let tapped_before = state.players[actor.index()]
            .battlefield
            .iter()
            .filter(|id| state.objects.get(**id).tapped)
            .count();
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, .. } if player == actor));
        engine::step(&mut state, Action::CastSpell(talent)).unwrap();
        for _ in 0..24 {
            let decision = engine::advance_until_decision(&mut state);
            if state.stack.is_empty() && state.objects.get(talent).zone == Zone::Battlefield {
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
                );
                break;
            }
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.objects.get(talent).zone, Zone::Battlefield);
        assert_eq!(crate::standard_cards_v1::class_level(&state, talent), 1);
        for (ability, level) in [(0, 2), (1, 3)] {
            engine::step(&mut state, Action::ActivateAbility(talent, ability)).unwrap();
            for _ in 0..24 {
                let decision = engine::advance_until_decision(&mut state);
                if state.stack.is_empty() && state.engine.pending_activation.is_none() {
                    assert!(
                        matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
                    );
                    break;
                }
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert_eq!(crate::standard_cards_v1::class_level(&state, talent), level);
        }
        let tapped_after = state.players[actor.index()]
            .battlefield
            .iter()
            .filter(|id| state.objects.get(**id).tapped)
            .count();
        assert_eq!(
            tapped_after - tapped_before,
            7,
            "printed cast + level 2 + level 3 costs"
        );
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert!(!requires_successor(&state), "genuine Talent without Season");
        let mut restored = restore(&state);
        for world in [&mut state, &mut restored] {
            crate::standard_creatures_v1::place_counters_checked_v1(
                world,
                recipient,
                actor,
                CounterTransferV1 {
                    extras: CounterExtrasV1 {
                        loyalty: 1_100_000_000,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                crate::planeswalker_v1::loyalty(world, recipient),
                Some(3_300_000_000)
            );
            let before = world.clone();
            assert!(crate::standard_creatures_v1::place_counters_checked_v1(
                world,
                overflow,
                actor,
                CounterTransferV1 {
                    extras: CounterExtrasV1 {
                        loyalty: 1_500_000_000,
                        ..Default::default()
                    },
                    ..Default::default()
                }
            )
            .is_none());
            assert_eq!(*world, before);
            assert!(crate::standard_creatures_v1::place_counters_checked_v1(
                world,
                overflow,
                actor,
                CounterTransferV1 {
                    counters: crate::state::Counters {
                        charge: i32::MAX,
                        ..Default::default()
                    },
                    ..Default::default()
                }
            )
            .is_none());
            assert_eq!(*world, before);
        }
        assert_eq!(state, restored);
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}
