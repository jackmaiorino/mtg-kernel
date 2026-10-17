//! Genuine native SPG75 Partial fixtures. No owner/model/transport admission.
use super::*;
use crate::{
    card_def::{self, CardCapability},
    engine::{self, Action, Decision},
    event::{CommittedEvent, ProposedEvent},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};

fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}

fn ready(actor: PlayerId, opponent_library: &[&str]) -> GameState {
    let own = vec![id("Island"); 48];
    let other = opponent_library
        .iter()
        .map(|name| id(name))
        .collect::<Vec<_>>();
    let libraries = if actor == PlayerId::P0 {
        [&own[..], &other[..]]
    } else {
        [&other[..], &own[..]]
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        libraries[0],
        libraries[1],
        crate::rl::card_name,
        110_466,
        actor,
    );
    state.step = Step::Main1;
    for player in [actor, actor.opponent()] {
        for _ in 0..12 {
            put(&mut state, player, "Island", Zone::Battlefield);
        }
        for _ in 0..4 {
            put(&mut state, player, "Forest", Zone::Battlefield);
        }
    }
    // These are original declared hand/setup cards, not post-draw invention.
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor, "Island", Zone::Hand);
    state
}

fn respond(state: &mut GameState, decision: Decision, reverse: bool) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::ChooseTargets {
            player,
            ref legal_targets,
            ..
        } => {
            let target = Target::Player(player.opponent());
            assert!(legal_targets.contains(&target));
            Action::ChooseTarget(target)
        }
        Decision::ChooseEffectTargets {
            ref legal_targets,
            can_finish: false,
            ..
        } => {
            let target = if reverse {
                legal_targets.last()
            } else {
                legal_targets.first()
            }
            .copied()
            .unwrap();
            Action::ChooseEffectTarget(target)
        }
        Decision::Discard {
            count: 1, choices, ..
        } => Action::Discard(vec![choices[0]]),
        other => panic!("Tutelage native fixture decision: {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
}

fn finish(state: &mut GameState, reverse: bool) {
    for _ in 0..256 {
        let decision = engine::advance_until_decision(state);
        assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_discard.is_none()
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        respond(state, decision, reverse);
    }
    panic!("finite original library did not finish Tutelage");
}

fn taps(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|object| {
            let live = state.objects.get(**object);
            live.tapped && card_def::CARD_DEFS[usize::from(live.card_def)].is_land
        })
        .count()
}

fn paid_tutelage(state: &mut GameState, actor: PlayerId) -> ObjectId {
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    let source = put(state, actor, "Sphinx's Tutelage", Zone::Hand);
    let before = taps(state, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.stack.last().unwrap().source, source);
    assert_eq!(
        state.stack.last().unwrap().kind,
        crate::state::StackItemKind::Spell
    );
    assert_eq!(state.objects.get(source).zone, Zone::Stack);
    assert_eq!(taps(state, actor) - before, 3);
    finish(state, false);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(!state.objects.get(source).tapped);
    source
}

fn activate_to_discard(state: &mut GameState, actor: PlayerId, source: ObjectId) {
    let decision = engine::advance_until_decision(state);
    if matches!(decision, Decision::CastSpellOrPass { player, .. } if player != actor) {
        engine::step(state, Action::Pass).unwrap();
    }
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    let before = taps(state, actor);
    let hand = state.players[actor.index()].hand.len();
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert!(state.engine.pending_activation.is_none());
    let ability = state
        .stack
        .last()
        .expect("genuine paid activation was published");
    assert_eq!(ability.source, source);
    assert_eq!(ability.controller, actor);
    assert_eq!(ability.kind, crate::state::StackItemKind::ActivatedAbility);
    assert_eq!(ability.v4.activated_ability_index, Some(0));
    assert_eq!(
        ability.v4.ability_source_contract.unwrap().controller,
        actor
    );
    assert_eq!(taps(state, actor) - before, 6);
    assert!(!state.objects.get(source).tapped);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    for _ in 0..12 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::Discard { player, count: 1, .. } if player == actor) {
            assert_eq!(state.players[actor.index()].hand.len(), hand + 1);
            assert!(state
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event, CommittedEvent::SphinxsTutelageDrawV1 { .. })));
            return;
        }
        respond(state, decision, false);
    }
    panic!("paid 5U draw did not reach its genuine discard");
}

fn to_mill_order(state: &mut GameState) {
    for _ in 0..24 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::ChooseEffectTargets { .. }) {
            assert!(matches!(
                state.engine.pending_effect.as_ref().unwrap().choice,
                Some(PendingEffectChoice::SelectTargets {
                    purpose: EffectTargetSelectionPurpose::SphinxsTutelageMillOrderV1 { .. },
                    ..
                })
            ));
            validate_pending_effect_choice(state).unwrap();
            return;
        }
        respond(state, decision, false);
    }
    panic!("Tutelage mill ordering missing");
}

fn restore(state: &GameState) -> GameState {
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(&restored, state);
    assert_eq!(restored.state_hash(), state.state_hash());
    restored
}

#[test]
fn exact_partial_printing_and_paid_activation_pair_stops_both_seats() {
    let definition = &card_def::CARD_DEFS[usize::from(id("Sphinx's Tutelage"))];
    assert_eq!(id("Sphinx's Tutelage"), 466);
    assert_eq!(definition.capability, CardCapability::Partial);
    assert_eq!(definition.mana_value, 3);
    assert_eq!(
        (definition.spell_effect)(),
        Some(EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield
        })
    );
    assert_eq!(definition.activated_abilities.len(), 1);
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (cards, milled) in [
            (vec![], 0),
            (vec!["Elvish Mystic"], 1),
            (vec!["Forest", "Elvish Mystic", "Island"], 2),
            (vec!["Elvish Mystic", "Think Twice", "Island"], 2),
            (vec!["Soulstone Sanctuary", "Ravenous Amulet", "Forest"], 2),
            (vec!["Ravenous Amulet", "Viridian Longbow", "Forest"], 2),
            (
                vec![
                    "Zimone, Paradox Sculptor",
                    "Elvish Mystic",
                    "Forest",
                    "Island",
                ],
                4,
            ),
            (vec!["Think Twice", "Think Twice", "Elvish Mystic"], 3),
            (
                vec![
                    "Elvish Mystic",
                    "Llanowar Elves",
                    "Think Twice",
                    "Think Twice",
                ],
                4,
            ),
        ] {
            let mut state = ready(actor, &cards);
            if milled == 4 {
                // Authentic existing permanent setup exercises the native
                // damage-modifier router. Only Tutelage is claimed paid here.
                put(&mut state, actor, "Twinflame Tyrant", Zone::Battlefield);
            }
            let source = paid_tutelage(&mut state, actor);
            // The V53 appendix is not a whole-world legacy consumer gate.
            assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
            activate_to_discard(&mut state, actor, source);
            let before_discard = restore(&state);
            let discard = engine::advance_until_decision(&mut state);
            respond(&mut state, discard, false);
            let mut replay = before_discard;
            let discard = engine::advance_until_decision(&mut replay);
            respond(&mut replay, discard, false);
            finish(&mut state, false);
            finish(&mut replay, false);
            assert_eq!(state, replay);
            assert_eq!(
                state.players[actor.opponent().index()].graveyard.len(),
                milled
            );
            assert_eq!(
                state.players[actor.opponent().index()].library.len(),
                cards.len() - milled
            );
            assert_eq!(state.players[actor.index()].graveyard.len(), 1);
            assert_eq!(state.players[actor.index()].hand.len(), 2);
            assert!(state.players[actor.index()]
                .graveyard
                .iter()
                .all(|object| !state.players[actor.index()].library.contains(object)));
        }
    }
}

#[test]
fn paid_repeat_original_pair_order_restore_and_readonly_forgery_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(
            actor,
            &[
                "Zimone, Paradox Sculptor",
                "Elvish Mystic",
                "Think Twice",
                "Think Twice",
                "Forest",
            ],
        );
        let source = paid_tutelage(&mut state, actor);
        activate_to_discard(&mut state, actor, source);
        let discard = engine::advance_until_decision(&mut state);
        respond(&mut state, discard, false);
        to_mill_order(&mut state);
        let original = restore(&state);
        let pending = original.engine.pending_effect.as_ref().unwrap().clone();
        for kind in 0..10 {
            let mut forged = original.clone();
            let continuation = forged.engine.pending_effect.as_mut().unwrap();
            match kind {
                0 => continuation.ctx.controller = actor.opponent(),
                1 => continuation.ctx.source = ObjectId(u32::MAX),
                2 => continuation.ctx.targets[0] = Target::Player(actor),
                3 => {
                    continuation
                        .resolving_item
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                4 => continuation.choice = None,
                5 => continuation.frames.push(EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                    path: vec![],
                }),
                6 => {
                    if let Some(PendingEffectChoice::SelectTargets { path, .. }) =
                        continuation.choice.as_mut()
                    {
                        path.push(0);
                    }
                }
                7 => {
                    if let Some(PendingEffectChoice::SelectTargets { player, .. }) =
                        continuation.choice.as_mut()
                    {
                        *player = actor;
                    }
                }
                8 => {
                    if let Some(PendingEffectChoice::SelectTargets { legal, .. }) =
                        continuation.choice.as_mut()
                    {
                        legal.reverse();
                    }
                }
                9 => {
                    if let Some(PendingEffectChoice::SelectTargets {
                        purpose:
                            EffectTargetSelectionPurpose::SphinxsTutelageMillOrderV1 {
                                previous_batch,
                                ..
                            },
                        ..
                    }) = continuation.choice.as_mut()
                    {
                        *previous_batch = Some(0);
                    }
                }
                _ => unreachable!(),
            }
            let before = forged.clone();
            assert!(
                validate_pending_effect_choice(&forged).is_err(),
                "forgery {kind}"
            );
            assert_eq!(forged, before);
        }
        let mut unowned = original.clone();
        assert!(resume_resumable_resolution(&mut unowned).is_err());
        assert_eq!(unowned, original);
        let mut replay = restore(&original);
        // Reverse the physical graveyard order, retaining the original color pair.
        let decision = engine::advance_until_decision(&mut state);
        respond(&mut state, decision, true);
        let decision = engine::advance_until_decision(&mut replay);
        respond(&mut replay, decision, true);
        assert_eq!(state, replay);
        validate_pending_effect_choice(&state).unwrap();
        assert_eq!(
            pending.resolving_item,
            state.engine.pending_effect.as_ref().unwrap().resolving_item
        );
        finish(&mut state, true);
        finish(&mut replay, true);
        assert_eq!(state, replay);
        assert_eq!(state.players[actor.opponent().index()].graveyard.len(), 5);
    }
}

#[test]
fn paid_trigger_survives_genuine_broken_wings_response_and_original_controller_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Elvish Mystic", "Llanowar Elves", "Forest"]);
        let source = paid_tutelage(&mut state, actor);
        activate_to_discard(&mut state, actor, source);
        let discard = engine::advance_until_decision(&mut state);
        respond(&mut state, discard, false);
        // Announce its mandatory opponent target, then expose the real response window.
        for _ in 0..12 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
            {
                break;
            }
            respond(&mut state, decision, false);
        }
        let root = state.stack.last().unwrap().clone();
        assert!(matches!(
            root.inline_effect,
            Some(EffectOp::SphinxsTutelageMillV1 { .. })
        ));
        let wings = put(&mut state, actor.opponent(), "Broken Wings", Zone::Hand);
        let before = taps(&state, actor.opponent());
        engine::step(&mut state, Action::CastSpell(wings)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&Target::Object(source)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        engine::advance_until_decision(&mut state);
        assert_eq!(taps(&state, actor.opponent()) - before, 3);
        let mut replay = restore(&state);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.players[actor.opponent().index()].graveyard.len(), 4); // Wings plus three actual mills.
        assert_eq!(root.controller, actor);
    }
}

#[test]
fn event_time_draw_membership_control_and_generation_native_primitives_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 12]);
        let source = paid_tutelage(&mut state, actor);
        let original = crate::state::AbilitySourceContractV4::capture(&state, source);
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        // Genuine native control primitive, not a purported paid control spell.
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor.opponent()));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor.opponent()));
        let pending = crate::trigger::capture_triggers_without_sba(&mut state);
        let own = pending
            .iter()
            .filter(|trigger| matches!(trigger.effect, EffectOp::SphinxsTutelageMillV1 { .. }))
            .collect::<Vec<_>>();
        assert_eq!(own.len(), 2);
        assert_eq!(own[0].source_contract, Some(original));
        assert_eq!(own[0].controller, actor);
        assert_eq!(own[1].controller, actor.opponent());
        assert_eq!(
            own[1].source_contract.unwrap().zone_change_count,
            original.zone_change_count
        );
        assert_eq!(
            state.objects.get(source).zone_change_count,
            original.zone_change_count + 1
        );
        // A new entry cannot retroactively observe a previous draw.
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        assert!(crate::trigger::capture_triggers_without_sba(&mut state)
            .iter()
            .all(|trigger| !matches!(trigger.effect, EffectOp::SphinxsTutelageMillV1 { .. })));
        assert_eq!(restore(&state), state);
    }
}

#[test]
fn no_observer_and_empty_failed_draw_preserve_old_event_shape() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &[]);
        // A normal selected-catalog turn anchor can already precede this draw.
        let original = state.engine.event_history.clone();
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        assert_eq!(
            &state.engine.event_history[..original.len()],
            original.as_slice()
        );
        assert_eq!(state.engine.event_history.len(), original.len() + 1);
        assert!(matches!(
            state.engine.event_history[original.len()],
            CommittedEvent::Draw {
                object: Some(_),
                ..
            }
        ));
        let source = paid_tutelage(&mut state, actor);
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        let before = state.engine.event_history.len();
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor.opponent()));
        assert!(matches!(
            state.engine.event_history[before..],
            [CommittedEvent::Draw { object: None, .. }]
        ));
        assert!(state.players[actor.opponent().index()].drew_from_empty);
    }
}
#[test]
fn paid_repeat_receipt_pair_and_full_library_corruption_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(
            actor,
            &[
                "Elvish Mystic",
                "Llanowar Elves",
                "Think Twice",
                "Think Twice",
                "Forest",
            ],
        );
        let source = paid_tutelage(&mut state, actor);
        activate_to_discard(&mut state, actor, source);
        let discard = engine::advance_until_decision(&mut state);
        respond(&mut state, discard, false);
        to_mill_order(&mut state);
        // The pair's last remaining member is forced and appended by the
        // genuine first ordering action. Do not advance into the next pair.
        let decision = engine::advance_until_decision(&mut state);
        respond(&mut state, decision, true);
        let answered = restore(&state);
        validate_pending_effect_choice(&answered).unwrap();
        let mut changed_order = answered.clone();
        if let EffectFrame::SphinxsTutelageMillV1 {
            ordered: Some(ordered),
            ..
        } = changed_order
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .frames
            .last_mut()
            .unwrap()
        {
            ordered.reverse();
        } else {
            panic!("real selected mill pair has no answered frame");
        }
        let unchanged = changed_order.clone();
        assert!(validate_pending_effect_choice(&changed_order).is_err());
        assert_eq!(changed_order, unchanged);
        to_mill_order(&mut state);
        let original = restore(&state);
        assert_eq!(state.players[actor.opponent().index()].graveyard.len(), 2);
        let batch = original
            .engine
            .event_history
            .iter()
            .position(|event| matches!(event, CommittedEvent::SphinxsTutelageMillBatchV1 { .. }))
            .unwrap();
        for kind in 0..8 {
            let mut forged = original.clone();
            match kind {
                0 => {
                    forged.engine.event_history.remove(batch);
                }
                1 => {
                    let duplicate = forged.engine.event_history[batch].clone();
                    forged.engine.event_history.push(duplicate);
                }
                2 => {
                    if let CommittedEvent::SphinxsTutelageMillBatchV1 { ordered, .. } =
                        &mut forged.engine.event_history[batch]
                    {
                        ordered.reverse();
                    }
                }
                3 => {
                    if let CommittedEvent::SphinxsTutelageMillBatchV1 { previous_batch, .. } =
                        &mut forged.engine.event_history[batch]
                    {
                        *previous_batch = Some(batch as u64);
                    }
                }
                4 => {
                    if let CommittedEvent::SphinxsTutelageMillBatchV1 { move_start, .. } =
                        &mut forged.engine.event_history[batch]
                    {
                        *move_start += 1;
                    }
                }
                5 => {
                    forged.engine.event_history.swap(batch - 1, batch - 2);
                }
                6 => {
                    forged.players[actor.opponent().index()].library.swap(0, 1);
                }
                7 => {
                    let moved = forged.players[actor.opponent().index()].graveyard[0];
                    forged.objects.get_mut(moved).zone_change_count += 1;
                }
                _ => unreachable!(),
            }
            let before = forged.clone();
            assert!(
                validate_pending_effect_choice(&forged).is_err(),
                "batch forgery {kind}"
            );
            assert_eq!(forged, before);
        }
        let mut replay = restore(&original);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(state.players[actor.opponent().index()].graveyard.len(), 5);
    }
}
#[test]
fn genuine_paid_counterspell_keeps_cast_costs_and_grants_no_ability_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 8]);
        let source = put(&mut state, actor, "Sphinx's Tutelage", Zone::Hand);
        let before = taps(&state, actor);
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        assert!(state.engine.pending_cast.is_none());
        assert_eq!(state.stack.last().unwrap().source, source);
        assert_eq!(taps(&state, actor) - before, 3);
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
        );
        let counter = put(&mut state, actor.opponent(), "Counterspell", Zone::Hand);
        let opposing_taps = taps(&state, actor.opponent());
        engine::step(&mut state, Action::CastSpell(counter)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&Target::Object(source)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        engine::advance_until_decision(&mut state);
        assert_eq!(taps(&state, actor.opponent()) - opposing_taps, 2);
        let mut replay = restore(&state);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(taps(&state, actor) - before, 3);
        let before = state.clone();
        assert!(engine::step(&mut state, Action::ActivateAbility(source, 0)).is_err());
        assert_eq!(state, before);
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event, CommittedEvent::SphinxsTutelageDrawV1 { .. })));
    }
}

#[test]
fn paid_deep_analysis_captures_each_real_draw_then_orders_two_native_triggers_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 8]);
        let source = paid_tutelage(&mut state, actor);
        let spell = put(&mut state, actor, "Deep Analysis", Zone::Hand);
        let before = taps(&state, actor);
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&Target::Player(actor)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Player(actor))).unwrap();
        engine::advance_until_decision(&mut state);
        assert_eq!(taps(&state, actor) - before, 4);
        let mut saw_order = false;
        for _ in 0..12 {
            let decision = engine::advance_until_decision(&mut state);
            if let Decision::OrderTriggers {
                player,
                ref pending,
            } = decision
            {
                assert_eq!(player, actor);
                assert_eq!(pending.len(), 2);
                assert!(pending
                    .iter()
                    .all(|trigger| trigger.source == source && trigger.controller == actor));
                let captures = pending
                    .iter()
                    .map(|trigger| match trigger.effect {
                        EffectOp::SphinxsTutelageMillV1 {
                            capture_history_index,
                        } => capture_history_index,
                        _ => panic!("Tutelage real draw lost its captured root"),
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(captures.len(), 2);
                let saved = restore(&state);
                engine::step(&mut state, Action::OrderTriggers(vec![1, 0])).unwrap();
                let mut replay = saved;
                engine::step(&mut replay, Action::OrderTriggers(vec![1, 0])).unwrap();
                finish(&mut state, false);
                finish(&mut replay, false);
                assert_eq!(state, replay);
                saw_order = true;
                break;
            }
            respond(&mut state, decision, false);
        }
        assert!(saw_order);
        assert_eq!(state.players[actor.opponent().index()].graveyard.len(), 4);
        assert_eq!(
            state
                .engine
                .event_history
                .iter()
                .filter(|event| matches!(event, CommittedEvent::SphinxsTutelageDrawV1 { .. }))
                .count(),
            2
        );
    }
}

#[test]
fn paid_published_mill_changed_root_and_erased_stages_refuse_public_apis_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Elvish Mystic", "Llanowar Elves", "Forest"]);
        let source = paid_tutelage(&mut state, actor);
        // This is the original genuine 5U draw/discard activation. Its own
        // stack incarnation is not selected by the published mill receipt.
        activate_to_discard(&mut state, actor, source);
        let discard = engine::advance_until_decision(&mut state);
        respond(&mut state, discard, false);
        to_mill_order(&mut state);
        let waiting = restore(&state);
        // The pair's last remaining member is forced and appended by the
        // genuine first ordering action. Do not advance into the next pair.
        let decision = engine::advance_until_decision(&mut state);
        respond(&mut state, decision, false);
        let answered = restore(&state);
        assert!(answered
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .is_none());
        assert!(answered
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .answered_choice_guard
            .is_some());
        for original in [waiting, answered] {
            validate_pending_effect_choice(&original).unwrap();
            let original_pending = original.engine.pending_effect.as_ref().unwrap();
            let ctx = original_pending.ctx.clone();
            let mut changed_item = original_pending.resolving_item.clone();
            let foreign = EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            };
            changed_item.inline_effect = Some(foreign.clone());
            let mut forged = restore(&original);
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            pending.resolving_item = changed_item.clone();
            pending.choice = None;
            pending.frames = vec![EffectFrame::Program {
                op: foreign.clone(),
                path: vec![],
            }];
            pending.answered_choice_guard = None;
            let before = forged.clone();
            assert!(validate_pending_effect_choice(&forged).is_err());
            assert!(resume_resumable_resolution(&mut forged).is_err());
            assert_eq!(forged, before);
            assert!(acquire_tutelage_resume_v1(&forged, &changed_item).is_err());
            assert_eq!(forged, before);
            execute(&foreign, &ctx, &mut forged);
            assert_eq!(
                forged.engine.halted,
                Some((
                    engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source
                ))
            );
            forged.engine.halted = None;
            assert_eq!(forged, before);
            // Remove the pending stage only for this readonly attack on begin.
            // The actual published native row and its receipt stay unchanged.
            forged.engine.pending_effect = None;
            let before_begin = forged.clone();
            assert!(
                begin_resumable_resolution(&foreign, &ctx, changed_item.clone(), &mut forged)
                    .is_err()
            );
            assert_eq!(forged, before_begin);
            assert!(acquire_tutelage_begin_v1(&forged, &changed_item).is_err());
            assert_eq!(forged, before_begin);
            for erase_contract in [false, true] {
                let mut unknown_ctx = ctx.clone();
                unknown_ctx.stack_item_id = Some(crate::ids::StackItemId(u64::MAX));
                let mut unknown_item = changed_item.clone();
                unknown_item.v4.stack_item_id = unknown_ctx.stack_item_id.unwrap();
                unknown_item.kind = crate::state::StackItemKind::Spell;
                if erase_contract {
                    unknown_ctx.ability_source_contract = None;
                    unknown_item.v4.ability_source_contract = None;
                }
                let mut unknown = restore(&original);
                let pending = unknown.engine.pending_effect.as_mut().unwrap();
                pending.ctx = unknown_ctx.clone();
                pending.resolving_item = unknown_item.clone();
                pending.choice = None;
                pending.frames = vec![EffectFrame::Program {
                    op: foreign.clone(),
                    path: vec![],
                }];
                pending.answered_choice_guard = None;
                let before = unknown.clone();
                assert!(validate_pending_effect_choice(&unknown).is_err());
                assert!(resume_resumable_resolution(&mut unknown).is_err());
                assert_eq!(unknown, before);
                execute(&foreign, &unknown_ctx, &mut unknown);
                assert_eq!(
                    unknown.engine.halted,
                    Some((
                        engine::UnsupportedMechanic::InvalidEffectContinuation,
                        source
                    ))
                );
                unknown.engine.halted = None;
                assert_eq!(unknown, before);
                unknown.engine.pending_effect = None;
                let before = unknown.clone();
                assert!(begin_resumable_resolution(
                    &foreign,
                    &unknown_ctx,
                    unknown_item.clone(),
                    &mut unknown
                )
                .is_err());
                assert_eq!(unknown, before);
                assert!(acquire_native_execution_owner_v1(&unknown, &unknown_item).is_err());
                assert_eq!(unknown, before);
            }
            let mut genuine = original.clone();
            let mut replay = restore(&original);
            finish(&mut genuine, false);
            finish(&mut replay, false);
            assert_eq!(genuine, replay);
            assert_eq!(genuine.players[actor.opponent().index()].graveyard.len(), 3);
        }
    }
}

#[test]
fn paid_draw_discard_above_published_mill_keeps_lexical_native_owner_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest", "Island", "Forest", "Island", "Forest"]);
        let source = paid_tutelage(&mut state, actor);
        activate_to_discard(&mut state, actor, source);
        let discard = engine::advance_until_decision(&mut state);
        respond(&mut state, discard, false);
        let mut waiting = None;
        for _ in 0..24 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
                waiting = state
                    .stack
                    .iter()
                    .find(|row| tutelage_item_v1(&state, row))
                    .cloned();
                if waiting.is_some() {
                    break;
                }
            }
            respond(&mut state, decision, false);
        }
        let waiting = waiting.expect("genuine first mill is published before its response window");
        event::validate_published_tutelage_v1(&state, &waiting).unwrap();
        // Catalog-only entry probes do not require an enclosing Runner for
        // this separate genuine draw/discard activation above the mill row.
        assert!(crate::linked_aura_operation_v1::needs_event_owner(&state));
        assert!(!crate::linked_aura_operation_v1::needs_surrounding_operation_owner(&state));
        let before = taps(&state, actor);
        activate_to_discard(&mut state, actor, source);
        assert_eq!(taps(&state, actor) - before, 6);
        assert!(state.stack.iter().any(|row| row == &waiting));
        assert!(state.engine.pending_effect.is_none());
        let discard = state.engine.pending_discard.as_ref().unwrap();
        assert_eq!((discard.player, discard.count), (actor, 1));
        let engine::DiscardResume::FinishAbilityResolution { stack_item_id } = &discard.resume
        else {
            panic!("actual terminal discard retains its resolving activation row")
        };
        let ability = state
            .stack
            .iter()
            .find(|row| row.v4.stack_item_id == *stack_item_id)
            .unwrap();
        assert_eq!(ability.kind, crate::state::StackItemKind::ActivatedAbility);
        assert_ne!(ability.v4.stack_item_id, waiting.v4.stack_item_id);
        assert_eq!(ability.source, source);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            let decision = engine::advance_until_decision(branch);
            assert!(
                matches!(decision, Decision::Discard { player, count: 1, .. } if player == actor)
            );
            respond(branch, decision, false);
        }
        assert_eq!(state, replay);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(state.players[actor.opponent().index()].graveyard.len(), 4);
        assert_eq!(taps(&state, actor), 15);
        assert!(!state.objects.get(source).tapped);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    }
}
