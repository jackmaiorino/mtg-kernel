use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v6::tests::put;
use crate::state::{GameState, StackItemKind, Target, Zone};

fn paid(actor: PlayerId, owner: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
        put(&mut state, player, "Mountain", Zone::Battlefield);
    }
    let target = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
    paid_on(state, actor, target)
}

fn paid_on(
    mut state: GameState,
    actor: PlayerId,
    target: ObjectId,
) -> (GameState, ObjectId, ObjectId) {
    let spell = put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
    let definition = state.objects.get(spell).card_def;
    assert_eq!(
        CARD_DEFS[usize::from(definition)].capability,
        crate::card_def::CardCapability::Partial
    );
    assert!(spell_custody::selected_definition(definition));
    assert!(crate::card_def::preflight_fully_supported_deck(&[definition]).is_err());
    for root in [
        crate::rules_meaning_fake_your_own_death_v50::RootV50::InstantSpell,
        crate::rules_meaning_fake_your_own_death_v50::RootV50::GainedDiesAbility,
    ] {
        let meaning =
            crate::rules_meaning_fake_your_own_death_v50::printed_meaning(definition, root)
                .unwrap();
        crate::rules_meaning_fake_your_own_death_v50::validate(&meaning).unwrap();
        assert_eq!(meaning.capability, crate::card_def::CardCapability::Partial);
        assert!(!meaning.model_admitted);
    }
    for _ in 0..2 {
        let swamp = put(&mut state, actor, "Swamp", Zone::Battlefield);
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, mana_abilities, .. }
            if player == actor && mana_abilities.contains(&swamp))
        );
        engine::step(&mut state, Action::ActivateManaAbility(swamp)).unwrap();
        assert!(state.objects.get(swamp).tapped);
    }
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
        2
    );
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { player, legal_targets, .. }
        if player == actor && legal_targets.contains(&Target::Object(target)))
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
        0
    );
    assert_eq!(state.stack.last().unwrap().source, spell);
    assert!(state.engine.event_history.iter().any(|event|
        matches!(event, CommittedEvent::FakeJournalV50 { journal } if matches!(journal.as_ref(), spell_custody::JournalV50::Published { item } if item.source == spell))));
    public_facts(&state);
    (state, spell, target)
}

fn public_facts(state: &GameState) -> public_v50::FactsV50 {
    let facts = public_v50::capture(state).unwrap();
    public_v50::validate(state, &facts).unwrap();
    let decoded: public_v50::FactsV50 =
        serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap();
    assert_eq!(facts, decoded);
    facts
}

fn drain(state: &mut GameState) {
    for _ in 0..24 {
        if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
            public_facts(state);
            return;
        }
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
            public_facts(state);
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual paid Fake stack did not finish")
}

fn death(state: &mut GameState, target: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(target, Zone::Graveyard));
    assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(
        state.stack.last().unwrap().kind,
        StackItemKind::TriggeredAbility
    );
    assert!(matches!(
        state.stack.last().unwrap().inline_effect,
        Some(EffectOp::FakeDeathFromCaptureV50 { .. })
    ));
}

fn treasures(state: &GameState, player: PlayerId) -> usize {
    let definition = treasure_definition().unwrap();
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|object| {
            state.objects.get(**object).card_def == definition
                && state.objects.get(**object).v4.is_token
        })
        .count()
}

#[test]
fn actual_partial_printing_pays_1b_and_returns_once_with_death_controller_treasure_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for owner in [actor, actor.opponent()] {
            let (mut state, spell, target) = paid(actor, owner);
            drain(&mut state);
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(engine::effective_power(&state, target), 3);
            assert_eq!(spell_custody::installed_grants(&state).unwrap().len(), 1);
            death(&mut state, target);
            let generation = state.objects.get(target).zone_change_count;
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            for game in [&mut state, &mut restored] {
                drain(game);
                spell_custody::validate_retained_grant_history(game).unwrap();
                let returned = game.objects.get(target);
                assert_eq!(
                    (
                        returned.zone,
                        returned.owner,
                        returned.controller,
                        returned.tapped
                    ),
                    (Zone::Battlefield, owner, owner, true)
                );
                assert_eq!(returned.zone_change_count, generation + 1);
                assert_eq!(engine::effective_power(game, target), 1);
                assert_eq!(treasures(game, owner), 1);
                assert_eq!(treasures(game, owner.opponent()), 0);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn actual_paid_spell_and_gained_trigger_counter_have_distinct_no_grant_and_no_return_outcomes() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut spell_countered, spell, target) = paid(actor, actor);
        let id = spell_countered.stack.last().unwrap().v4.stack_item_id;
        engine::counter_stack_item_by_id(&mut spell_countered, id).unwrap();
        assert_eq!(spell_countered.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(engine::effective_power(&spell_countered, target), 1);
        assert!(spell_custody::installed_grants(&spell_countered)
            .unwrap()
            .is_empty());
        spell_custody::validate_retained_grant_history(&spell_countered).unwrap();

        let (mut trigger_countered, _, target) = paid(actor, actor);
        drain(&mut trigger_countered);
        death(&mut trigger_countered, target);
        let id = trigger_countered.stack.last().unwrap().v4.stack_item_id;
        engine::counter_stack_item_by_id(&mut trigger_countered, id).unwrap();
        spell_custody::validate_retained_grant_history(&trigger_countered).unwrap();
        assert_eq!(trigger_countered.objects.get(target).zone, Zone::Graveyard);
        assert_eq!(treasures(&trigger_countered, actor), 0);
    }
}

#[test]
fn actual_removed_resolution_revalidates_capture_and_collection_before_any_write() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for capture in [true, false] {
            let (mut state, _, target) = paid(actor, actor);
            drain(&mut state);
            death(&mut state, target);
            let item = state.stack.last().unwrap().clone();
            let owner = death_custody::acquire_resolution(&state, &item)
                .unwrap()
                .unwrap();
            assert_eq!(state.stack.pop().unwrap(), item);
            let index = state
                .engine
                .event_history
                .iter()
                .position(|event| {
                    matches!(event,
                    CommittedEvent::FakeJournalV50 { journal } if if capture {
                        matches!(journal.as_ref(), spell_custody::JournalV50::DeathCaptured { .. })
                    } else {
                        matches!(journal.as_ref(), spell_custody::JournalV50::DeathCollected { .. })
                    })
                })
                .unwrap();
            state.engine.event_history[index] = CommittedEvent::Tap { object: target };
            let mut context = ExecCtx::no_targets(item.source, item.controller);
            context.stack_item_id = Some(item.v4.stack_item_id);
            context.ability_source_contract = item.v4.ability_source_contract;
            let before = serde_json::to_vec(&state).unwrap();
            assert!(death_custody::resolve_owned(&mut state, owner, &item, &context).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}

#[test]
fn actual_paid_fake_banner_return_borrows_original_color_row_and_restores_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
            put(&mut state, player, "Mountain", Zone::Battlefield);
        }
        let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
        for _ in 0..3 {
            let swamp = put(&mut state, actor, "Swamp", Zone::Battlefield);
            engine::advance_until_decision(&mut state);
            engine::step(&mut state, Action::ActivateManaAbility(swamp)).unwrap();
        }
        engine::step(&mut state, Action::CastSpell(banner)).unwrap();
        reach_banner_color(&mut state, actor);
        engine::step(&mut state, Action::ChooseEffectOption(4)).unwrap();
        assert_eq!(
            crate::heraldic_banner_v1::authenticated_color(&state, banner).unwrap(),
            ManaColor::G
        );

        // Exercise the existing targeted animation primitive with captured source
        // and target contracts. This is an operand fixture, not a paid Tough
        // Cookie printing or Full-card admission.
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let mut context = ExecCtx::no_targets(source, actor);
        context.ability_source_contract = Some(crate::state::AbilitySourceContractV4::capture(
            &state, source,
        ));
        context.targets.push(Target::Object(banner));
        context
            .target_contracts
            .push(crate::state::StackTargetContractV4::capture(
                &state,
                Target::Object(banner),
            ));
        crate::standard_creatures_v1::execute(
            crate::standard_creatures_v1::CreatureEffectV1::ToughCookieAnimate,
            &context,
            &mut state,
        );
        assert!(engine::object_has_type(
            &state,
            banner,
            crate::card_def::CardType::Creature
        ));
        let (mut state, _, target) = paid_on(state, actor, banner);
        drain(&mut state);
        death(&mut state, target);
        let original = state.stack.last().unwrap().clone();
        reach_banner_color(&mut state, actor);
        assert_eq!(state.stack.last(), Some(&original));
        assert_eq!(state.objects.get(banner).zone, Zone::Graveyard);
        assert_eq!(treasures(&state, actor), 0);
        spell_custody::validate_retained_grant_history(&state).unwrap();
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut state, &mut restored] {
            let before = serde_json::to_vec(game).unwrap();
            assert!(engine::step(game, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(game).unwrap(), before);
            engine::step(game, Action::ChooseEffectOption(2)).unwrap();
            drain(game);
            assert_eq!(game.objects.get(banner).zone, Zone::Battlefield);
            assert!(game.objects.get(banner).tapped);
            assert!(!engine::object_has_type(
                game,
                banner,
                crate::card_def::CardType::Creature
            ));
            assert_eq!(
                crate::heraldic_banner_v1::authenticated_color(game, banner).unwrap(),
                ManaColor::B
            );
            assert_eq!(treasures(game, actor), 1);
            assert!(!game
                .stack
                .iter()
                .any(|row| row.v4.stack_item_id == original.v4.stack_item_id));
            spell_custody::validate_retained_grant_history(game).unwrap();
            let mut reintroduced = game.clone();
            reintroduced.stack.push(original.clone());
            assert!(spell_custody::validate_retained_grant_history(&reintroduced).is_err());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        for kind in 0..4 {
            let mut forged: GameState = serde_json::from_slice(&bytes).unwrap();
            if kind == 0 {
                forged.stack.last_mut().unwrap().controller = actor.opponent();
            } else if kind == 1 {
                forged.stack.push(original.clone());
            } else if kind == 2 {
                forged.engine.linked_aura_operation_v1 = Default::default();
            } else {
                forged.stack.clear();
            }
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(engine::step(&mut forged, Action::ChooseEffectOption(2)).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
    }
}

fn reach_banner_color(state: &mut GameState, actor: PlayerId) {
    for _ in 0..24 {
        match engine::advance_until_decision(state) {
            Decision::ChooseEntryColorV36 { player, colors, .. } => {
                assert_eq!(player, actor);
                assert_eq!(colors, crate::heraldic_banner_v1::COLORS);
                return;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("actual Banner color choice missing: {other:?}"),
        }
    }
    panic!("actual Banner color choice did not arrive")
}

#[test]
fn actual_paid_fake_aura_zero_single_multiple_untargeted_hosts_return_tapped_before_treasure() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for owner in [actor, actor.opponent()] {
            for host_count in [0, 1, 2] {
                let mut state = crate::extra_turn_v1::tests::ready(actor);
                for player in [PlayerId::P0, PlayerId::P1] {
                    put(&mut state, player, "Lightning Bolt", Zone::Hand);
                    put(&mut state, player, "Mountain", Zone::Battlefield);
                }
                let original_host = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
                let aura = put(&mut state, owner, "Witness Protection", Zone::Battlefield);
                state
                    .attach_object_exact(
                        aura,
                        state.objects.get(aura).zone_change_count,
                        original_host,
                        state.objects.get(original_host).zone_change_count,
                    )
                    .unwrap();
                // The fixture starts with the existing incarnation-bound animation
                // operand. It does not claim a paid animation printing in this pool.
                let timestamp = engine::next_timestamp(&mut state);
                state.objects.get_mut(aura).v4.creature_upgrade =
                    Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                        temporary_creature: Some((4, 4, timestamp)),
                        ..Default::default()
                    });
                assert!(engine::object_has_type(
                    &state,
                    aura,
                    crate::card_def::CardType::Creature
                ));
                let (mut state, _, _) = paid_on(state, actor, aura);
                drain(&mut state);
                let protected = put(
                    &mut state,
                    owner.opponent(),
                    "Guardian of the Guildpact",
                    Zone::Battlefield,
                );
                let ward_host = (host_count == 2).then(|| {
                    put(
                        &mut state,
                        owner.opponent(),
                        "Tolarian Terror",
                        Zone::Battlefield,
                    )
                });
                if host_count == 0 {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(original_host, Zone::Exile),
                    );
                } else {
                    crate::effect::install_temporary_boost(
                        &mut state,
                        crate::effect::EffectObjectBinding {
                            object: original_host,
                            expected_zone: Zone::Battlefield,
                            expected_zone_change_count: 0,
                        },
                        0,
                        0,
                        crate::card_def::Keywords::HEXPROOF | crate::card_def::Keywords::SHROUD,
                    );
                }
                death(&mut state, aura);
                let original = state.stack.last().unwrap().clone();
                let target_events = state
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| matches!(event, CommittedEvent::Targeted { .. }))
                    .count();
                if host_count == 2 {
                    let mut found = false;
                    for _ in 0..24 {
                        match engine::advance_until_decision(&mut state) {
                            Decision::ChooseEffectTargets {
                                player,
                                source,
                                legal_targets,
                                min_targets,
                                max_targets,
                                can_finish,
                                ..
                            } => {
                                assert_eq!(
                                    (player, source, min_targets, max_targets, can_finish),
                                    (owner, aura, 1, 1, false)
                                );
                                assert_eq!(
                                    legal_targets,
                                    vec![
                                        Target::Object(original_host),
                                        Target::Object(ward_host.unwrap())
                                    ]
                                );
                                assert!(!legal_targets.contains(&Target::Object(protected)));
                                found = true;
                                break;
                            }
                            Decision::CastSpellOrPass { .. } => {
                                engine::step(&mut state, Action::Pass).unwrap()
                            }
                            other => panic!("actual untargeted Aura owner missing: {other:?}"),
                        }
                    }
                    assert!(found);
                    assert_eq!(state.stack.last(), Some(&original));
                    assert_eq!(treasures(&state, owner), 0);
                    let mut backend =
                        crate::rl_session::FastActorSessionV1::from_fake_v50_fixture_state(
                            state.clone(),
                        );
                    let crate::rl_session::FastActorResponseV1::Decision(e) =
                        backend.current_response()
                    else {
                        panic!("actual V50 mandatory Aura input");
                    };
                    let (observation, actions, binding) = backend
                        .fake_current_decision_input_v50(e, owner.into())
                        .unwrap();
                    assert_eq!(actions.len(), 2);
                    assert_eq!(
                        observation
                            .fake
                            .aura_menu
                            .as_ref()
                            .unwrap()
                            .legal_hosts
                            .len(),
                        2
                    );
                    assert!(!actions.iter().any(|action| matches!(
                        action,
                        crate::rl::ActionSemanticV1::FinishEffectSelection { .. }
                    )));
                    let before = backend.privileged_core_environment_hash();
                    backend
                        .fake_validate_bound_decision_v50(e, owner.into(), &binding)
                        .unwrap();
                    assert!(backend
                        .fake_validate_bound_decision_v50(e, owner.opponent().into(), &binding)
                        .is_err());
                    assert_eq!(backend.privileged_core_environment_hash(), before);
                    let snapshot = backend.snapshot_v1();
                    let index = actions.iter().position(|action| matches!(action, crate::rl::ActionSemanticV1::ChooseEffectTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == selected.0)).unwrap();
                    backend.step(e.episode_id, e.step, index as u32).unwrap();
                    let final_state = serde_json::to_vec(backend.fake_fixture_state()).unwrap();
                    backend.fake_restore_v50(&snapshot).unwrap();
                    let crate::rl_session::FastActorResponseV1::Decision(restored_e) =
                        backend.current_response()
                    else {
                        panic!("actual V50 restored Aura input");
                    };
                    assert!(restored_e.environment_revision > e.environment_revision);
                    assert!(backend
                        .fake_validate_bound_decision_v50(e, owner.into(), &binding)
                        .is_err());
                    backend
                        .step(restored_e.episode_id, restored_e.step, index as u32)
                        .unwrap();
                    assert_eq!(
                        serde_json::to_vec(backend.fake_fixture_state()).unwrap(),
                        final_state
                    );
                    let facts = public_facts(&state);
                    let menu = facts.aura_menu.unwrap();
                    assert_eq!(menu.owner, owner.into());
                    assert!(menu.mandatory);
                    assert!(!menu.targeted);
                    assert_eq!(
                        menu.legal_hosts
                            .iter()
                            .map(|host| host.arena_id)
                            .collect::<Vec<_>>(),
                        vec![original_host.0, ward_host.unwrap().0]
                    );
                    assert_eq!(
                        facts.deaths[menu.death].phase,
                        public_v50::DeathPhaseV50::ResolvingEntry
                    );
                }
                let bytes = serde_json::to_vec(&state).unwrap();
                let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
                let selected = ward_host.unwrap_or(original_host);
                for game in [&mut state, &mut restored] {
                    if host_count == 2 {
                        let before = serde_json::to_vec(game).unwrap();
                        assert!(engine::step(game, Action::FinishEffectSelection).is_err());
                        assert_eq!(serde_json::to_vec(game).unwrap(), before);
                        engine::step(game, Action::ChooseEffectTarget(Target::Object(selected)))
                            .unwrap();
                    }
                    drain(game);
                    spell_custody::validate_retained_grant_history(game).unwrap();
                    assert_eq!(treasures(game, owner), 1);
                    assert_eq!(treasures(game, owner.opponent()), 0);
                    assert_eq!(
                        game.engine
                            .event_history
                            .iter()
                            .filter(|event| matches!(event, CommittedEvent::Targeted { .. }))
                            .count(),
                        target_events
                    );
                    let object = game.objects.get(aura);
                    if host_count == 0 {
                        assert_eq!(object.zone, Zone::Graveyard);
                        assert_eq!(object.v4.attached_to, None);
                    } else {
                        assert_eq!(
                            (object.zone, object.owner, object.controller, object.tapped),
                            (Zone::Battlefield, owner, owner, true)
                        );
                        assert_eq!(
                            object.v4.attached_to,
                            Some(crate::state::ObjectLinkV4 {
                                object: selected,
                                zone_change_count: game.objects.get(selected).zone_change_count
                            })
                        );
                        assert!(game.objects.get(selected).attachments.contains(&aura));
                        assert!(!engine::object_has_type(
                            game,
                            aura,
                            crate::card_def::CardType::Creature
                        ));
                    }
                    let inputs = game
                        .engine
                        .event_history
                        .iter()
                        .enumerate()
                        .filter_map(|(at, event)| match spell_custody::journal(event) {
                            Some(spell_custody::JournalV50::AuraReturnInput { input }) => {
                                Some((at, input))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(inputs.len(), 1);
                    assert_eq!(inputs[0].1.choice.owner, owner);
                    assert_eq!(inputs[0].1.choice.candidates.len(), host_count);
                    let completed = game
                        .engine
                        .event_history
                        .iter()
                        .enumerate()
                        .find_map(|(at, event)| match spell_custody::journal(event) {
                            Some(spell_custody::JournalV50::AuraReturnCompleted {
                                selected,
                                returned,
                                attachment,
                                ..
                            }) => Some((at, *selected, *returned, *attachment)),
                            _ => None,
                        })
                        .unwrap();
                    let treasure_input = game
                        .engine
                        .event_history
                        .iter()
                        .position(|event| {
                            matches!(
                                spell_custody::journal(event),
                                Some(spell_custody::JournalV50::DeathTreasureInput { .. })
                            )
                        })
                        .unwrap();
                    assert!(completed.0 < treasure_input);
                    assert_eq!(completed.1.is_some(), host_count != 0);
                    if let (Some(movement), Some(relation)) = (completed.2, completed.3) {
                        assert!(
                            inputs[0].0 < movement as usize
                                && movement < relation
                                && relation < completed.0 as u64
                        );
                    } else {
                        assert_eq!(host_count, 0);
                    }
                    let mut erased = game.clone();
                    for event in &mut erased.engine.event_history {
                        if matches!(
                            spell_custody::journal(event),
                            Some(
                                spell_custody::JournalV50::AuraReturnInput { .. }
                                    | spell_custody::JournalV50::AuraAttachmentCommitted { .. }
                                    | spell_custody::JournalV50::AuraReturnCompleted { .. }
                            )
                        ) {
                            *event = CommittedEvent::Tap { object: aura };
                        }
                    }
                    let before = serde_json::to_vec(&erased).unwrap();
                    assert!(engine::step(&mut erased, Action::Pass).is_err());
                    assert_eq!(serde_json::to_vec(&erased).unwrap(), before);
                    if host_count == 0 {
                        let mut reordered = game.clone();
                        let completion = std::mem::replace(
                            &mut reordered.engine.event_history[completed.0],
                            CommittedEvent::Tap { object: aura },
                        );
                        reordered.engine.event_history.push(completion);
                        let before = serde_json::to_vec(&reordered).unwrap();
                        assert!(engine::step(&mut reordered, Action::Pass).is_err());
                        assert_eq!(serde_json::to_vec(&reordered).unwrap(), before);
                    }
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
                if host_count == 2 {
                    for kind in 0..4 {
                        let mut forged: GameState = serde_json::from_slice(&bytes).unwrap();
                        let pending = forged
                            .engine
                            .linked_aura_operation_v1
                            .pending_fake_aura_v50
                            .as_mut()
                            .unwrap();
                        if kind == 2 {
                            pending.original.stack.clear();
                        } else {
                            let crate::linked_aura_operation_v1::GeneralChoiceV36::FakeAuraV50 {
                                choice,
                                ..
                            } = &mut pending.choice
                            else {
                                panic!("actual Aura checkpoint");
                            };
                            if kind == 0 {
                                choice.owner = owner.opponent();
                            } else if kind == 1 {
                                choice.candidates.pop();
                            } else {
                                choice.aura.expected_zone_change_count += 1;
                            }
                        }
                        let before = serde_json::to_vec(&forged).unwrap();
                        assert!(engine::step(
                            &mut forged,
                            Action::ChooseEffectTarget(Target::Object(selected))
                        )
                        .is_err());
                        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
                    }
                }
            }
        }
    }
}

#[test]
fn actual_paid_fake_returns_opponent_owned_card_but_treasure_belongs_to_death_controller() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let owner = actor.opponent();
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
            put(&mut state, player, "Mountain", Zone::Battlefield);
        }
        let target = put(&mut state, owner, "Llanowar Elves", Zone::Hand);
        // A real owner/controller-separated entry initializes the fixture.
        // This does not claim an additional paid control-changing printing.
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(target, actor),
        );
        assert_eq!(
            (
                state.objects.get(target).owner,
                state.objects.get(target).controller
            ),
            (owner, actor)
        );
        let (mut state, _, _) = paid_on(state, actor, target);
        drain(&mut state);
        death(&mut state, target);
        assert_eq!(state.stack.last().unwrap().controller, actor);
        let original = state.stack.last().unwrap().clone();
        let source = public_v50::stack_source_reference(&state, &original)
            .unwrap()
            .unwrap();
        assert_eq!(
            (source.zone, source.controller, source.owner),
            (Zone::Battlefield, actor.into(), owner.into())
        );
        assert_eq!(state.objects.get(target).controller, owner);
        let mut forged = original.clone();
        forged.controller = owner;
        let before = serde_json::to_vec(&state).unwrap();
        assert!(public_v50::stack_source_reference(&state, &forged).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut state, &mut restored] {
            drain(game);
            spell_custody::validate_retained_grant_history(game).unwrap();
            let returned = game.objects.get(target);
            assert_eq!(
                (
                    returned.zone,
                    returned.owner,
                    returned.controller,
                    returned.tapped
                ),
                (Zone::Battlefield, owner, owner, true)
            );
            assert_eq!(treasures(game, actor), 1);
            assert_eq!(treasures(game, owner), 0);
            let facts = public_facts(game);
            assert_eq!(facts.deaths[0].controller_at_death, actor.into());
            assert_eq!(facts.deaths[0].returned_controller, Some(owner.into()));
            assert_eq!(facts.deaths[0].treasure_controller, Some(actor.into()));
            assert_eq!(facts.deaths[0].treasures.len(), 1);
            let mut forged = facts.clone();
            forged.deaths[0].treasure_controller = Some(owner.into());
            let before = serde_json::to_vec(game).unwrap();
            assert!(public_v50::validate(game, &forged).is_err());
            assert_eq!(serde_json::to_vec(game).unwrap(), before);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn actual_paid_fake_lost_target_does_not_follow_its_returned_incarnation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let (mut state, spell, target) = paid(actor, actor.opponent());
            let original = state.objects.get(target).zone_change_count;
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Exile));
            if reenter {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Battlefield),
                );
                assert!(state.objects.get(target).zone_change_count > original);
            }
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            for game in [&mut state, &mut restored] {
                drain(game);
                assert_eq!(game.objects.get(spell).zone, Zone::Graveyard);
                assert!(spell_custody::installed_grants(game).unwrap().is_empty());
                assert_eq!(treasures(game, actor), 0);
                assert_eq!(treasures(game, actor.opponent()), 0);
                assert!(game.engine.event_history.iter().any(|event| matches!(
                    spell_custody::journal(event),
                    Some(spell_custody::JournalV50::RemovalInput {
                        removal: spell_custody::RemovalV50::Resolving {
                            target_legal: false
                        },
                        ..
                    })
                )));
                spell_custody::validate_retained_grant_history(game).unwrap();
                if reenter {
                    assert_eq!(engine::effective_power(game, target), 1);
                }
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn actual_paid_fake_graveyard_response_prevents_return_but_preserves_treasure() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, target) = paid(actor, actor.opponent());
        drain(&mut state);
        death(&mut state, target);
        let original = state.stack.last().unwrap().clone();
        // The response operand uses the real zone-change path. It does not
        // claim another paid graveyard-exile printing.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Exile));
        assert_eq!(state.stack.last(), Some(&original));
        let generation = state.objects.get(target).zone_change_count;
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut state, &mut restored] {
            drain(game);
            assert_eq!(game.objects.get(target).zone, Zone::Exile);
            assert_eq!(game.objects.get(target).zone_change_count, generation);
            assert_eq!(treasures(game, actor.opponent()), 1);
            assert_eq!(treasures(game, actor), 0);
            assert!(game.engine.event_history.iter().any(|event| matches!(
                spell_custody::journal(event),
                Some(spell_custody::JournalV50::DeathResolutionClosed { returned: None, created, .. })
                    if created.len() == 1
            )));
            spell_custody::validate_retained_grant_history(game).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn actual_paid_fake_copied_token_death_creates_treasure_without_returning_token() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
            put(&mut state, player, "Mountain", Zone::Battlefield);
        }
        let original = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        // The existing token-copy constructor initializes the operand and
        // records actual creation. No paid token-maker printing is claimed.
        let token =
            crate::standard_cards_v1::create_token_copy(&mut state, original, actor).unwrap();
        assert!(state.objects.get(token).v4.is_token);
        let (mut state, _, _) = paid_on(state, actor, token);
        drain(&mut state);
        death(&mut state, token);
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut state, &mut restored] {
            drain(game);
            assert!(!game.players[actor.index()].battlefield.contains(&token));
            assert!(!game.players[actor.opponent().index()]
                .battlefield
                .contains(&token));
            assert_eq!(treasures(game, actor), 1);
            assert_eq!(treasures(game, actor.opponent()), 0);
            assert!(game.engine.event_history.iter().any(|event| matches!(
                spell_custody::journal(event),
                Some(spell_custody::JournalV50::DeathCaptured { lookback, .. })
                    if lookback.was_token_at_death
            )));
            assert!(game.engine.event_history.iter().any(|event| matches!(
                spell_custody::journal(event),
                Some(spell_custody::JournalV50::DeathResolutionClosed { returned: None, created, .. })
                    if created.len() == 1
            )));
            spell_custody::validate_retained_grant_history(game).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn actual_paid_fake_power_and_gained_death_ability_expire_at_real_cleanup() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, target) = paid(actor, actor);
        drain(&mut state);
        for player in [PlayerId::P0, PlayerId::P1] {
            for _ in 0..40 {
                put(&mut state, player, "Forest", Zone::Library);
            }
        }
        let grant = spell_custody::installed_grants(&state).unwrap()[0];
        assert!(grant.active(&state).unwrap());
        assert_eq!(engine::effective_power(&state, target), 3);
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut state, &mut restored] {
            crate::policy_observation_v6::tests::advance_to_next_main_for(game, actor.opponent());
            assert!(!grant.active(game).unwrap());
            assert!(!grant.temporary_modifier_retained(game).unwrap());
            assert_eq!(engine::effective_power(game, target), 1);
            assert_eq!(
                game.engine
                    .event_history
                    .iter()
                    .filter(|event| matches!(
                        spell_custody::journal(event),
                        Some(spell_custody::JournalV50::CleanupCompleted { .. })
                    ))
                    .count(),
                1
            );
            event::propose_and_commit(game, ProposedEvent::zone_change(target, Zone::Graveyard));
            assert!(matches!(
                engine::advance_until_decision(game),
                Decision::CastSpellOrPass { .. }
            ));
            assert!(game.stack.is_empty());
            assert_eq!(game.objects.get(target).zone, Zone::Graveyard);
            assert_eq!(treasures(game, actor), 0);
            assert!(!game.engine.event_history.iter().any(|event| matches!(
                spell_custody::journal(event),
                Some(spell_custody::JournalV50::DeathCaptured { .. })
            )));
            spell_custody::validate_retained_grant_history(game).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn actual_paid_fake_copied_spell_publication_retargets_and_restores_without_losing_birth() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, spell, original_target) = paid(actor, actor);
        let copy_target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        // The shared engine constructor initializes a genuine copy of the
        // paid spell. It does not claim a second paid copy-making printing.
        let (copy_source, copy_id) =
            engine::copy_spell_for(&mut state, spell, actor.opponent()).unwrap();
        let born = public_facts(&state);
        assert_eq!(born.spells.len(), 1);
        assert_eq!(born.unfinalized_copies.len(), 1);
        assert_eq!(born.unfinalized_copies[0].source.arena_id, copy_source.0);
        assert_eq!(
            born.unfinalized_copies[0].original_target.arena_id,
            original_target.0
        );
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut state, &mut restored] {
            engine::finish_spell_copy_targets(game, copy_id, Some(Target::Object(copy_target)))
                .unwrap();
            let finalized = public_facts(game);
            assert!(finalized.unfinalized_copies.is_empty());
            assert_eq!(finalized.spells.len(), 2);
            assert!(finalized.spells[1].copied);
            assert_eq!(finalized.spells[1].copy_occurrence, Some(0));
            assert_eq!(finalized.spells[1].original_target.arena_id, copy_target.0);
            drain(game);
            assert_eq!(engine::effective_power(game, original_target), 3);
            assert_eq!(engine::effective_power(game, copy_target), 3);
            death(game, original_target);
            drain(game);
            death(game, copy_target);
            drain(game);
            let facts = public_facts(game);
            assert_eq!(facts.grants.len(), 2);
            assert_eq!(facts.deaths.len(), 2);
            assert_eq!(facts.deaths[0].treasure_controller, Some(actor.into()));
            assert_eq!(
                facts.deaths[1].treasure_controller,
                Some(actor.opponent().into())
            );
            assert_eq!(treasures(game, actor), 1);
            assert_eq!(treasures(game, actor.opponent()), 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn actual_paid_fake_public_facts_mask_face_down_recipient_and_ignore_unrelated_hidden_hand() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
            put(&mut state, player, "Mountain", Zone::Battlefield);
        }
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let hidden_definition = state.objects.get(target).card_def;
        // Existing face-down state initializes the privacy operand. No paid
        // Disguise printing is claimed by this Fake fixture.
        state.objects.get_mut(target).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: true,
            lookers: 1 << actor.index(),
            hidden_by: None,
            hidden_by_source: None,
        });
        let (mut state, _, _) = paid_on(state, actor, target);
        let sentinel = crate::card_def::card_id_by_name("Face-down creature").unwrap();
        let facts = public_facts(&state);
        assert_eq!(facts.spells[0].original_target.card_db_id, sentinel);
        assert_ne!(sentinel, hidden_definition);
        let mut other = state.clone();
        let hidden_hand = other.players[actor.opponent().index()].hand[0];
        let replacement = crate::card_def::card_id_by_name("Mountain").unwrap();
        other.objects.get_mut(hidden_hand).card_def = replacement;
        other.objects.get_mut(hidden_hand).name = "Mountain".into();
        assert_eq!(facts, public_facts(&other));
        drain(&mut state);
        assert_eq!(
            public_facts(&state).grants[0].original_recipient.card_db_id,
            sentinel
        );
        death(&mut state, target);
        drain(&mut state);
        let revealed = public_facts(&state);
        assert_eq!(
            revealed.spells[0].original_target.card_db_id,
            hidden_definition
        );
        assert_eq!(
            revealed.deaths[0].original_creature.card_db_id,
            hidden_definition
        );
        assert_eq!(
            revealed.deaths[0].phase,
            public_v50::DeathPhaseV50::Resolved
        );
        assert_eq!(treasures(&state, actor), 1);
    }
}

#[test]
fn actual_two_paid_fake_grants_hold_distinct_original_sources_and_return_once_with_two_treasures() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, target) = paid(actor, actor);
        drain(&mut state);
        let (mut state, _, _) = paid_on(state, actor, target);
        drain(&mut state);
        assert_eq!(engine::effective_power(&state, target), 5);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        let Decision::OrderTriggers { player, pending } =
            engine::advance_until_decision(&mut state)
        else {
            panic!("two real gained death abilities must require ordering");
        };
        assert_eq!(player, actor);
        assert_eq!(pending.len(), 2);
        let held = state.engine.pending_triggers.clone();
        assert_eq!(held.len(), 2);
        assert_ne!(held[0].effect, held[1].effect);
        for trigger in &held {
            let source = public_v50::pending_source_reference(&state, trigger)
                .unwrap()
                .unwrap();
            assert_eq!(
                (
                    source.arena_id,
                    source.zone,
                    source.zone_change_count,
                    source.controller
                ),
                (target.0, Zone::Battlefield, 0, actor.into())
            );
            let mut forged = trigger.clone();
            forged.controller = actor.opponent();
            let before = serde_json::to_vec(&state).unwrap();
            assert!(public_v50::pending_source_reference(&state, &forged).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        let facts = public_facts(&state);
        assert_eq!(facts.deaths.len(), 2);
        assert_eq!(facts.trigger_order.len(), 2);
        assert!(facts
            .deaths
            .iter()
            .all(|death| death.phase == public_v50::DeathPhaseV50::Held));
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut state, &mut restored] {
            engine::step(game, Action::OrderTriggers(vec![1, 0])).unwrap();
            for item in &game.stack {
                if public_v50::selected(item) {
                    let reference = public_v50::stack_source_reference(game, item)
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        (reference.zone, reference.controller),
                        (Zone::Battlefield, actor.into())
                    );
                }
            }
            drain(game);
            assert_eq!(game.objects.get(target).zone, Zone::Battlefield);
            assert!(game.objects.get(target).tapped);
            assert_eq!(game.objects.get(target).zone_change_count, 2);
            assert_eq!(engine::effective_power(game, target), 1);
            assert_eq!(treasures(game, actor), 2);
            let facts = public_facts(game);
            assert_eq!(
                facts
                    .deaths
                    .iter()
                    .filter(|death| death.returned_incarnation.is_some())
                    .count(),
                1
            );
            assert_eq!(
                facts
                    .deaths
                    .iter()
                    .map(|death| death.treasures.len())
                    .sum::<usize>(),
                2
            );
            assert!(facts
                .deaths
                .iter()
                .all(|death| death.phase == public_v50::DeathPhaseV50::Resolved));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn legacy_consumer_refusal_depends_on_known_fake_facts_not_unknown_opponent_hand() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let hidden = put(
            &mut state,
            actor.opponent(),
            "Fake Your Own Death",
            Zone::Hand,
        );
        public_v50::require_legacy_visible(&state, Some(actor)).unwrap();
        let decision = crate::policy_surface_v5::PolicyDecisionV5::Surface(
            crate::surface::SurfaceDecision::Decision(engine::advance_until_decision(&mut state)),
        );
        assert_eq!(decision.actor(&state), Some(actor));
        let v1 = serde_json::to_vec(&crate::rl::observe_v1(&state, actor, 0).unwrap()).unwrap();
        let candidates = crate::rl::core_policy_action_candidates_conqueror_scoped_v49(
            &decision, &state, false, false, false,
        )
        .unwrap();
        let semantics = serde_json::to_vec(
            &candidates
                .iter()
                .map(|candidate| &candidate.semantic)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let original = crate::rl::policy_observation_extensions_panda_v47(&state, actor).unwrap();
        let mut alternate = state.clone();
        alternate.objects.get_mut(hidden).card_def =
            crate::card_def::card_id_by_name("Mountain").unwrap();
        alternate.objects.get_mut(hidden).name = "Mountain".into();
        assert_eq!(
            v1,
            serde_json::to_vec(&crate::rl::observe_v1(&alternate, actor, 0).unwrap()).unwrap()
        );
        let alternate_candidates = crate::rl::core_policy_action_candidates_conqueror_scoped_v49(
            &decision, &alternate, false, false, false,
        )
        .unwrap();
        assert_eq!(
            semantics,
            serde_json::to_vec(
                &alternate_candidates
                    .iter()
                    .map(|candidate| &candidate.semantic)
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        );
        assert_eq!(
            original,
            crate::rl::policy_observation_extensions_panda_v47(&alternate, actor).unwrap()
        );
        assert!(public_v50::require_legacy_visible(&state, Some(actor.opponent())).is_err());
        let before = serde_json::to_vec(&state).unwrap();
        assert!(crate::rl::observe_v1(&state, actor.opponent(), 0).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state
            .reveal_hand_card(actor, actor.opponent(), hidden)
            .unwrap();
        let before = serde_json::to_vec(&state).unwrap();
        assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
        assert!(
            crate::rl::core_policy_action_candidates_conqueror_scoped_v49(
                &decision, &state, false, false, false,
            )
            .is_err()
        );
        assert!(crate::rl::policy_observation_extensions_panda_v47(&state, actor).is_err());
        assert!(crate::rl::policy_observation_extensions_conqueror_v49(&state, actor).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);

        let (paid, _, _) = paid(actor, actor);
        let before = serde_json::to_vec(&paid).unwrap();
        assert!(crate::rl::observe_v1(&paid, actor, 0).is_err());
        assert!(crate::rl::policy_observation_extensions_panda_v47(&paid, actor).is_err());
        assert!(crate::rl::policy_observation_extensions_conqueror_v49(&paid, actor).is_err());
        assert_eq!(serde_json::to_vec(&paid).unwrap(), before);
        public_facts(&paid);
    }
}

#[test]
fn predecessor_candidate_builders_refuse_actual_affordable_fake_in_decision_actor_hand() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let spell = put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
        for _ in 0..2 {
            let swamp = put(&mut state, actor, "Swamp", Zone::Battlefield);
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, mana_abilities, .. }
                if player == actor && mana_abilities.contains(&swamp)));
            engine::step(&mut state, Action::ActivateManaAbility(swamp)).unwrap();
        }
        let actual = engine::advance_until_decision(&mut state);
        assert!(
            matches!(&actual, Decision::CastSpellOrPass { player, castable_spells, .. }
            if *player == actor && castable_spells.contains(&spell))
        );
        let decision = crate::policy_surface_v5::PolicyDecisionV5::Surface(
            crate::surface::SurfaceDecision::Decision(actual),
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
        assert!(
            crate::rl::core_policy_action_candidates_conqueror_scoped_v49(
                &decision, &state, false, false, false,
            )
            .is_err()
        );
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

fn mixed_observation(state: &mut GameState) -> crate::fake_observation_v50::FakeObservationV50 {
    use crate::fake_observation_v50 as observation;
    let mut surface = crate::policy_surface_v5::PolicySurfaceV5::new_with_priority_mode_v1(
        crate::surface_v2::PriorityModeV1::EngineWindowsV1,
        crate::surface_v2::SuppressionAuditMode::Off,
    );
    let origin = surface.next_decision(state).unwrap();
    let actor = origin.actor(state).unwrap();
    let substep = origin.substep();
    let before = serde_json::to_vec(state).unwrap();
    let value = observation::observe_v50(state, &surface, (actor, &origin), 0, 0, substep).unwrap();
    value.validate_artifact().unwrap();
    observation::validate_current(state, &surface, (actor, &origin), 0, 0, substep, &value)
        .unwrap();
    assert_eq!(value.fake, public_facts(state));
    let serialized = serde_json::to_vec(&value).unwrap();
    let decoded: observation::FakeObservationV50 = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(decoded, value);
    assert!(
        serde_json::from_slice::<crate::conqueror_observation_v49::ConquerorObservationV49>(
            &serialized,
        )
        .is_err()
    );
    assert!(crate::conqueror_observation_v49::observe_v49(
        state,
        &surface,
        (actor, &origin),
        0,
        0,
        substep,
    )
    .is_err());
    assert!(
        observation::observe_v50(state, &surface, (actor.opponent(), &origin), 0, 0, substep,)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
    value
}

#[test]
fn actual_paid_fake_mixed_observation_preserves_live_death_source_restore_and_real_outcomes() {
    use crate::fake_observation_v50 as observation;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let owner = actor.opponent();
        let (mut state, _, target) = paid(actor, owner);
        let published = mixed_observation(&mut state);
        assert_eq!(published.fake.spells.len(), 1);
        assert_eq!(
            published.fake.spells[0].phase,
            public_v50::SpellPhaseV50::Published
        );
        assert_eq!(
            published.projection.surface.stack.last().unwrap().source,
            published.fake.spells[0].source
        );
        drain(&mut state);
        let granted = mixed_observation(&mut state);
        assert_eq!(granted.fake.grants.len(), 1);
        assert!(granted.fake.grants[0].death_ability_active);
        death(&mut state, target);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let live = mixed_observation(&mut state);
        let restored_live = mixed_observation(&mut restored);
        assert_eq!(live, restored_live);
        assert_eq!(live.fake.deaths.len(), 1);
        let source = &live.projection.surface.stack.last().unwrap().source;
        assert_eq!(*source, live.fake.deaths[0].original_creature);
        assert_eq!(source.zone, Zone::Battlefield);
        assert_eq!(source.controller, owner.into());
        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);

        let mut surface = crate::policy_surface_v5::PolicySurfaceV5::new_with_engine_priority_v1();
        let origin = surface.next_decision(&mut state).unwrap();
        let seat = origin.actor(&state).unwrap();
        let mut forged = live.clone();
        forged.fake.spells[0].original_target.zone = Zone::Exile;
        forged.projection_sha256 = observation::commitment(&forged).unwrap();
        let before = serde_json::to_vec(&state).unwrap();
        assert!(observation::validate_current(
            &state,
            &surface,
            (seat, &origin),
            0,
            0,
            origin.substep(),
            &forged,
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);

        for game in [&mut state, &mut restored] {
            drain(game);
            let resolved = mixed_observation(game);
            assert_eq!(
                resolved.fake.deaths[0].phase,
                public_v50::DeathPhaseV50::Resolved
            );
            assert_eq!(
                resolved.fake.deaths[0].returned_controller,
                Some(owner.into())
            );
            assert_eq!(
                resolved.fake.deaths[0].treasure_controller,
                Some(owner.into())
            );
            assert_eq!(resolved.fake.deaths[0].treasures.len(), 1);
            assert_eq!(game.objects.get(target).zone, Zone::Battlefield);
            assert!(game.objects.get(target).tapped);
            assert_eq!(treasures(game, owner), 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn actual_paid_fake_mixed_observation_is_invariant_to_unrelated_unknown_opponent_hand() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, _) = paid(actor, actor);
        let mut surface = crate::policy_surface_v5::PolicySurfaceV5::new_with_engine_priority_v1();
        let origin = surface.next_decision(&mut state).unwrap();
        let observer = origin.actor(&state).unwrap();
        let hidden = *state.players[observer.opponent().index()]
            .hand
            .last()
            .unwrap();
        let mut alternate = state.clone();
        alternate.objects.get_mut(hidden).card_def =
            crate::card_def::card_id_by_name("Mountain").unwrap();
        alternate.objects.get_mut(hidden).name = "Mountain".into();
        assert_eq!(
            mixed_observation(&mut state),
            mixed_observation(&mut alternate)
        );
    }
}

#[test]
fn actual_paid_fake_history_coexists_with_paid_heartfire_crystal_historical_source_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, _) = paid(actor, actor);
        drain(&mut state);
        let source = put(&mut state, actor, "Heartfire Immolator", Zone::Hand);
        for name in ["Mountain", "Swamp"] {
            let land = put(&mut state, actor, name, Zone::Battlefield);
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, mana_abilities, .. }
                if player == actor && mana_abilities.contains(&land)));
            engine::step(&mut state, Action::ActivateManaAbility(land)).unwrap();
        }
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == actor && castable_spells.contains(&source)));
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        drain(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        let wall = put(
            &mut state,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        );
        let target = put(
            &mut state,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        // Existing typed prevention setup competes with the actual Barricade.
        // This does not claim an additional paid shield-producing printing.
        let shield = state.engine.next_replacement_id;
        state.engine.next_replacement_id += 1;
        state
            .engine
            .active_replacements
            .push(crate::event::ActiveReplacement {
                id: shield,
                source: wall,
                kind: crate::event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(target),
                    remaining: 1,
                },
            });
        let mountain = put(&mut state, actor, "Mountain", Zone::Battlefield);
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, mana_abilities, .. }
            if player == actor && mana_abilities.contains(&mountain)));
        engine::step(&mut state, Action::ActivateManaAbility(mountain)).unwrap();
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { player, legal_targets, .. }
            if player == actor && legal_targets.contains(&Target::Object(target))));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        for _ in 0..12 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(decision, Decision::ChooseEffectOption { player, source: actual, .. }
                if player == actor.opponent() && actual == source)
            {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        crate::effect::validate_pending_effect_choice(&state).unwrap();
        let item = state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .resolving_item
            .clone();
        assert!(matches!(
            item.inline_effect.as_ref(),
            Some(EffectOp::StandardV1(
                crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1
            ))
        ));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(crate::rl::akroma_crystal_resolving_source_ref_v40(&state, &item).is_err());
        let historical =
            crate::rl::akroma_crystal_resolving_source_ref_fake_scoped_v50(&state, &item, true)
                .unwrap();
        assert_eq!(historical.arena_id, source.0);
        assert_eq!(historical.zone, Zone::Battlefield);
        assert_eq!(historical.controller, actor.into());
        let mut forged = item.clone();
        forged.controller = actor.opponent();
        assert!(
            crate::rl::akroma_crystal_resolving_source_ref_fake_scoped_v50(&state, &forged, true,)
                .is_err()
        );
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let mut restored: GameState = serde_json::from_slice(&before).unwrap();
        assert_eq!(
            mixed_observation(&mut state),
            mixed_observation(&mut restored)
        );
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
