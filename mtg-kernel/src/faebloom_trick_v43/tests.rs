use super::*;
use crate::ids::PlayerId;

fn ready(actor: PlayerId) -> GameState {
    crate::extra_turn_v1::tests::ready(actor)
}
#[test]
fn absent_reflexive_history_preserves_state_wire_and_hash_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = ready(actor);
        let before = serde_json::to_vec(&state).unwrap();
        validate_state(&state).unwrap();
        assert!(!requires_successor(&state, Some(actor)));
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let restored: GameState = serde_json::from_slice(&before).unwrap();
        assert_eq!(restored.state_hash(), state.state_hash());
        assert!(state
            .engine
            .event_history
            .iter()
            .all(|e| journal(e).is_none()));
    }
}
#[test]
fn detached_creation_root_cannot_fabricate_tokens_or_reflexive_occurrence() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = state.players[actor.index()].hand[0];
        crate::effect::execute(
            &recipe(),
            &crate::effect::ExecCtx::no_targets(source, actor),
            &mut state,
        );
        assert!(state.engine.halted.is_some());
        assert!(state
            .engine
            .event_history
            .iter()
            .all(|e| journal(e).is_none()));
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, CommittedEvent::CreateToken { .. })));
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn registered_faerie_blueprint_is_exact_full_token_with_only_flying_and_no_printed_root() {
    let id = token_definition().unwrap();
    let definition = &CARD_DEFS[usize::from(id)];
    assert_eq!(definition.capability, crate::card_def::CardCapability::Full);
    assert_eq!(definition.types, [CardType::Creature]);
    assert_eq!(definition.subtypes, [crate::card_def::Subtype::Faerie]);
    assert_eq!(definition.colors, [crate::mana::ManaColor::U]);
    assert_eq!((definition.power, definition.toughness), (Some(1), Some(1)));
    assert_eq!(definition.keywords, crate::card_def::Keywords::FLYING);
    assert!(definition.keywords.has(crate::card_def::Keywords::FLYING));
    assert!((definition.spell_effect)().is_none());
    assert!(crate::trigger::triggers_for(id).is_empty());
    assert!(definition.activated_abilities.is_empty());
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    use crate::card_def::Keywords;
    use crate::engine::{self, Action, Decision};
    use crate::extra_turn_v1::tests::future_printed::put;
    use crate::mana::ManaColor;
    use crate::state::Step;

    fn next(state: &mut GameState) -> Decision {
        let decision = engine::advance_until_decision(state);
        assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
        decision
    }
    fn cast(state: &mut GameState, actor: PlayerId) -> StackItem {
        let source = put(state, actor, "Faebloom Trick", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        assert!(
            matches!(next(state),Decision::CastSpellOrPass {player,castable_spells,..}
            if player==actor && castable_spells.contains(&source))
        );
        engine::step(state, Action::CastSpell(source)).unwrap();
        let item = state.stack.last().unwrap().clone();
        assert!(item.targets.is_empty());
        assert_eq!(item.v4.target_spec, Some(TargetSpec::None));
        assert!(state.engine.pending_cast.is_none());
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        item
    }
    fn creation(state: &mut GameState) -> Decision {
        for _ in 0..12 {
            let decision = next(state);
            if state
                .engine
                .event_history
                .iter()
                .any(|e| matches!(journal(e), Some(JournalV43::Created { .. })))
            {
                return decision;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("actual spell did not create tokens")
    }
    fn finish(state: &mut GameState, id: StackItemId) {
        for _ in 0..12 {
            let d = next(state);
            if !state.stack.iter().any(|i| i.v4.stack_item_id == id) {
                validate_state(state).unwrap();
                return;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("actual reflexive did not complete")
    }
    fn refused(state: &GameState) {
        let wire = serde_json::to_vec(state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&wire).unwrap();
        assert!(engine::step(&mut restored, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&restored).unwrap(), wire);
        assert!(matches!(
            engine::advance_until_decision(&mut restored),
            Decision::Halted { .. }
        ));
        restored.engine.halted = None;
        assert_eq!(serde_json::to_vec(&restored).unwrap(), wire);
    }
    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn actual_targetless_cast_creates_two_then_separate_targeted_reflexive_with_restore_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let original = cast(&mut state, actor);
            let decision = creation(&mut state);
            assert_eq!(state.objects.get(original.source).zone, Zone::Graveyard);
            assert!(
                matches!(decision,Decision::ChooseTargets {player,remaining:1,legal_targets,..}
                if player==actor && legal_targets==vec![Target::Object(target)])
            );
            assert_eq!(state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::CreateToken {token_def,controller,..}
                if Some(*token_def)==crate::card_def::card_id_by_name("Faerie Token") && *controller==actor)).count(),2);
            assert!(!state.objects.get(target).tapped);
            let facts = crate::faebloom_public_v43::capture(&state).unwrap();
            assert_eq!(facts.spells.len(), 1);
            assert_eq!(facts.reflexives.len(), 1);
            assert_eq!(facts.spells[0].original_source.zone, Zone::Stack);
            assert_eq!(
                facts.spells[0].original_source.zone_change_count,
                original.v4.source_contract.unwrap().zone_change_count
            );
            assert_eq!(
                facts.reflexives[0].stage,
                crate::faebloom_public_v43::StageV43::Waiting
            );
            crate::faebloom_public_v43::validate(&state, &facts).unwrap();
            let mut forged = facts.clone();
            forged.reflexives[0].controller = actor.opponent().into();
            assert!(crate::faebloom_public_v43::validate(&state, &forged).is_err());
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                engine::step(branch, Action::ChooseTarget(Target::Object(target))).unwrap();
            }
            let row = state.stack.last().unwrap().clone();
            assert_eq!(row.kind, StackItemKind::TriggeredAbility);
            assert_eq!(row.v4.source_contract, original.v4.source_contract);
            assert!(!state.objects.get(target).tapped);
            finish(&mut state, row.v4.stack_item_id);
            finish(&mut restored, row.v4.stack_item_id);
            assert!(state.objects.get(target).tapped);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            let mut replay = state.clone();
            replay.stack.push(row);
            refused(&replay);
        }
    }
    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn actual_paid_spell_and_reflexive_counter_leave_distinct_creation_results_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let spell = cast(&mut state, actor);
            // Honest crate-local exact-ID counter primitive, not a printed counter spell claim.
            engine::counter_stack_item_by_id(&mut state, spell.v4.stack_item_id).unwrap();
            validate_state(&state).unwrap();
            assert!(!state
                .engine
                .event_history
                .iter()
                .any(|e| matches!(journal(e), Some(JournalV43::Created { .. }))));
            assert_eq!(state.objects.get(spell.source).zone, Zone::Graveyard);
            let mut state = ready(actor);
            let target2 = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            cast(&mut state, actor);
            creation(&mut state);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target2))).unwrap();
            let trigger = state.stack.last().unwrap().clone();
            engine::counter_stack_item_by_id(&mut state, trigger.v4.stack_item_id).unwrap();
            validate_state(&state).unwrap();
            assert!(!state.objects.get(target2).tapped);
            let mut forged = state.clone();
            forged.stack.push(trigger);
            refused(&forged);
            let _ = target;
        }
    }
    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn actual_no_opponent_or_shroud_hexproof_recipient_drops_only_reflexive_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for keyword in [None, Some(Keywords::SHROUD), Some(Keywords::HEXPROOF)] {
                let mut state = ready(actor);
                if let Some(keyword) = keyword {
                    let target = put(
                        &mut state,
                        actor.opponent(),
                        "Llanowar Elves",
                        Zone::Battlefield,
                    );
                    let binding = crate::effect::EffectObjectBinding {
                        object: target,
                        expected_zone: Zone::Battlefield,
                        expected_zone_change_count: state.objects.get(target).zone_change_count,
                    };
                    crate::effect::install_temporary_boost(&mut state, binding, 0, 0, keyword);
                }
                cast(&mut state, actor);
                creation(&mut state);
                validate_state(&state).unwrap();
                assert!(state.engine.pending_triggers.is_empty());
                assert!(state.stack.is_empty());
                assert_eq!(
                    state
                        .engine
                        .event_history
                        .iter()
                        .filter(|e| matches!(journal(e), Some(JournalV43::Created { .. })))
                        .count(),
                    1
                );
                assert_eq!(
                    state
                        .engine
                        .event_history
                        .iter()
                        .filter(|e| matches!(journal(e), Some(JournalV43::Dropped { .. })))
                        .count(),
                    1
                );
            }
        }
    }
    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn actual_waiting_and_published_erasure_or_fabricated_closure_preserves_journals_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            cast(&mut state, actor);
            creation(&mut state);
            let mut erased = state.clone();
            erased.engine.pending_triggers.clear();
            refused(&erased);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            let item = state.stack.last().unwrap().clone();
            let mut erased = state.clone();
            erased
                .stack
                .retain(|i| i.v4.stack_item_id != item.v4.stack_item_id);
            refused(&erased);
            let (pub_index, _) = publication_for(&state, &item).unwrap();
            // A manufactured completion cannot substitute an independently committed actual removal.
            emit(&mut erased, JournalV43::Countered { input: pub_index });
            refused(&erased);
            let mut actual = state.clone();
            engine::counter_stack_item_by_id(&mut actual, item.v4.stack_item_id).unwrap();
            let mut erased_input = actual.clone();
            for event in &mut erased_input.engine.event_history {
                if matches!(journal(event),Some(JournalV43::RemovalInput {item:row,..}) if row.v4.stack_item_id==item.v4.stack_item_id)
                {
                    *event = CommittedEvent::Tap { object: target };
                }
            }
            refused(&erased_input);
        }
    }
    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn actual_short_payment_and_target_departure_reentry_fizzle_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = put(&mut state, actor, "Faebloom Trick", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            let mut state = ready(actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            cast(&mut state, actor);
            creation(&mut state);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            let row = state.stack.last().unwrap().clone();
            // Labeled physical incarnation primitive at a real response checkpoint.
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Hand),
            );
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
            finish(&mut state, row.v4.stack_item_id);
            assert!(!state.objects.get(target).tapped);
            let facts = crate::faebloom_public_v43::capture(&state).unwrap();
            assert!(facts.reflexives[0].current_target.is_none());
            assert_eq!(
                facts.reflexives[0].stage,
                crate::faebloom_public_v43::StageV43::NoLegalTarget
            );
            assert!(state
                .engine
                .event_history
                .iter()
                .any(|e| matches!(journal(e), Some(JournalV43::NoLegalTarget { .. }))));
            assert_eq!(state.step, Step::Main1);
        }
    }
    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn actual_paid_spell_and_accepted_copy_have_distinct_creation_and_reflexive_controllers() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let opposing = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let original = cast(&mut state, actor);
            // Accepted exact spell-copy kernel producer from a genuinely paid live spell.
            // This fixture does not claim an additional printed copy card is admitted.
            let (copy_source, copy_id) =
                engine::copy_spell_for(&mut state, original.source, actor.opponent()).unwrap();
            let decision = creation(&mut state);
            assert!(
                matches!(decision,Decision::ChooseTargets {player,legal_targets,..}
                if player==actor.opponent() && legal_targets.contains(&Target::Object(own)))
            );
            assert!(!state
                .stack
                .iter()
                .any(|item| item.v4.stack_item_id == copy_id));
            assert!(state
                .stack
                .iter()
                .any(|item| item.v4.stack_item_id == original.v4.stack_item_id));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(own))).unwrap();
            let copied_reflexive = state.stack.last().unwrap().clone();
            assert_eq!(copied_reflexive.source, copy_source);
            assert_eq!(copied_reflexive.controller, actor.opponent());
            engine::counter_stack_item_by_id(&mut state, copied_reflexive.v4.stack_item_id)
                .unwrap();
            validate_state(&state).unwrap();
            assert!(!state.objects.get(own).tapped);
            // Wait for the original creation rather than treating the earlier copy receipt as new work.
            for _ in 0..12 {
                let decision = next(&mut state);
                let creations = state
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(journal(e), Some(JournalV43::Created { .. })))
                    .count();
                if creations == 2 {
                    assert!(
                        matches!(decision,Decision::ChooseTargets {player,legal_targets,..}
                        if player==actor && legal_targets.contains(&Target::Object(opposing)))
                    );
                    break;
                }
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                engine::step(&mut state, Action::Pass).unwrap();
            }
            engine::step(&mut state, Action::ChooseTarget(Target::Object(opposing))).unwrap();
            let original_reflexive = state.stack.last().unwrap().clone();
            assert_eq!(original_reflexive.controller, actor);
            assert_ne!(
                original_reflexive.v4.stack_item_id,
                copied_reflexive.v4.stack_item_id
            );
            finish(&mut state, original_reflexive.v4.stack_item_id);
            assert!(state.objects.get(opposing).tapped);
            let facts = crate::faebloom_public_v43::capture(&state).unwrap();
            assert_eq!(facts.spells.len(), 2);
            assert_eq!(facts.reflexives.len(), 2);
            assert_eq!(facts.spells.iter().filter(|s| s.copied_spell).count(), 1);
            assert_eq!(
                facts
                    .reflexives
                    .iter()
                    .filter(|r| r.stage == crate::faebloom_public_v43::StageV43::Countered)
                    .count(),
                1
            );
            assert_eq!(
                facts
                    .reflexives
                    .iter()
                    .filter(|r| r.stage == crate::faebloom_public_v43::StageV43::Tapped)
                    .count(),
                1
            );
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(
                crate::faebloom_public_v43::capture(&restored).unwrap(),
                facts
            );
            assert_eq!(
                state
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(e,CommittedEvent::CreateToken {token_def,..}
                if Some(*token_def)==crate::card_def::card_id_by_name("Faerie Token")))
                    .count(),
                4
            );
        }
    }

    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn actual_printed_an_offer_counter_before_creation_preserves_library_and_creates_real_treasures_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let spell = cast(&mut state, actor);
            // Retain actual caster priority before the opponent's real instant response.
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            engine::step(&mut state, Action::Pass).unwrap();
            let offer = put(
                &mut state,
                actor.opponent(),
                "An Offer You Can't Refuse",
                Zone::Hand,
            );
            state.players[actor.opponent().index()].mana_pool[ManaColor::U.pool_index()] = 1;
            let libraries = state.players.each_ref().map(|p| p.library.clone());
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. } if player == actor.opponent() && castable_spells.contains(&offer))
            );
            engine::step(&mut state, Action::CastSpell(offer)).unwrap();
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets == vec![Target::Object(spell.source)])
            );
            engine::step(
                &mut state,
                Action::ChooseTarget(Target::Object(spell.source)),
            )
            .unwrap();
            let response = state.stack.last().unwrap().v4.stack_item_id;
            let saved = serde_json::to_vec(&state).unwrap();
            finish(&mut state, response);
            assert_eq!(state.objects.get(spell.source).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(offer).zone, Zone::Graveyard);
            assert_eq!(
                state.players.each_ref().map(|p| p.library.clone()),
                libraries
            );
            assert!(state.engine.event_history.iter().all(|e| !matches!(
                journal(e),
                Some(JournalV43::Created { .. } | JournalV43::ReflexiveCaptured { .. })
            )));
            assert_eq!(state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::CreateToken { token_def, controller, .. } if Some(*token_def) == crate::card_def::card_id_by_name("Treasure Token") && *controller == actor)).count(), 2);
            let mut replay: GameState = serde_json::from_slice(&saved).unwrap();
            finish(&mut replay, response);
            assert_eq!(
                serde_json::to_vec(&replay).unwrap(),
                serde_json::to_vec(&state).unwrap()
            );
        }
    }

    #[test]
    #[ignore = "genuine unregistered Faebloom printing; parent serial admission required"]
    fn reflexive_blue_protection_control_and_historical_hidden_target_recheck_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for mode in 0..3 {
                let mut state = ready(actor);
                let target = put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                );
                if mode == 2 {
                    // A typed disguised-creature state, not a printed disguise-card claim.
                    state.objects.get_mut(target).v4.face_down_v1 =
                        Some(crate::state::FaceDownV1 {
                            disguised: true,
                            lookers: 0,
                            hidden_by: None,
                            hidden_by_source: None,
                        });
                }
                cast(&mut state, actor);
                creation(&mut state);
                engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
                let trigger = state.stack.last().unwrap().clone();
                let target_binding = crate::effect::EffectObjectBinding {
                    object: target,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: state.objects.get(target).zone_change_count,
                };
                if mode == 0 {
                    // Accepted temporary-keyword primitive proves the blue original spell source.
                    crate::effect::install_temporary_boost(
                        &mut state,
                        target_binding,
                        0,
                        0,
                        Keywords::PROTECTION_FROM_MONOCOLORED,
                    );
                } else if mode == 1 {
                    // Existing permanent-control kernel primitive, retaining original target generation.
                    crate::standard_cards_v1::gain_control(&mut state, target, actor);
                } else {
                    let facts = crate::faebloom_public_v43::capture(&state).unwrap();
                    let historical = facts.reflexives[0].target_at_placement.as_ref().unwrap();
                    assert_eq!(
                        Some(historical.card_db_id),
                        crate::card_def::card_id_by_name("Face-down creature")
                    );
                    let mut forged = state.clone();
                    for event in &mut forged.engine.event_history {
                        if let CommittedEvent::FaebloomJournalV43 { journal } = event {
                            if let JournalV43::TriggerPublished {
                                target_at_placement,
                                ..
                            } = journal.as_mut()
                            {
                                target_at_placement.card_db_id = state.objects.get(target).card_def;
                            }
                        }
                    }
                    refused(&forged);
                    crate::event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Hand),
                    );
                    let after = crate::faebloom_public_v43::capture(&state).unwrap();
                    assert_eq!(
                        after.reflexives[0].target_at_placement.as_ref(),
                        Some(historical)
                    );
                    assert!(after.reflexives[0].current_target.is_none());
                }
                let saved = serde_json::to_vec(&state).unwrap();
                finish(&mut state, trigger.v4.stack_item_id);
                assert!(!state.objects.get(target).tapped);
                assert_eq!(
                    crate::faebloom_public_v43::capture(&state)
                        .unwrap()
                        .reflexives[0]
                        .stage,
                    crate::faebloom_public_v43::StageV43::NoLegalTarget
                );
                let mut replay: GameState = serde_json::from_slice(&saved).unwrap();
                finish(&mut replay, trigger.v4.stack_item_id);
                assert_eq!(
                    serde_json::to_vec(&replay).unwrap(),
                    serde_json::to_vec(&state).unwrap()
                );
            }
        }
    }
}
