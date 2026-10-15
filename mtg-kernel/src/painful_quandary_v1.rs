//! Actual paid Partial-printing fixtures. Prepared only, not runtime-qualified.
use crate::{
    card_def::{self, CardCapability, CARD_DEFS},
    effect::{
        self, EffectAnsweredChoiceGuard, EffectFrame, EffectOp, EffectTargetSelectionPurpose,
        PendingEffectChoice,
    },
    engine::{self, Action, Decision},
    event::{self, CommittedEvent, ProposedEvent},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Target, Zone},
};

fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("player did not receive priority");
}

fn drain(state: &mut GameState) {
    for _ in 0..32 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
        {
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("paid stack did not resolve");
}

fn paid_quandary(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut state, actor, "Painful Quandary", Zone::Hand);
    let definition = state.objects.get(source).card_def;
    assert_eq!(
        CARD_DEFS[usize::from(definition)].capability,
        CardCapability::Full
    );
    assert!(card_def::preflight_fully_supported_deck(&[definition]).is_ok());
    for _ in 0..5 {
        let land = put(&mut state, actor, "Swamp", Zone::Battlefield);
        priority(&mut state, actor);
        engine::step(&mut state, Action::ActivateManaAbility(land)).unwrap();
        assert!(state.objects.get(land).tapped);
    }
    assert_eq!(
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()],
        5
    );
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.stack.last().unwrap().source, source);
    assert_eq!(state.stack.last().unwrap().kind, StackItemKind::Spell);
    assert!(state.engine.event_history.iter().any(|event| matches!(event, CommittedEvent::SpellCast { spell, controller } if *spell == source && *controller == actor)));
    assert_eq!(
        state.players[actor.index()]
            .mana_pool
            .iter()
            .copied()
            .map(u16::from)
            .sum::<u16>(),
        0
    );
    drain(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    (state, source)
}

fn cast_think_twice(state: &mut GameState, caster: PlayerId) -> ObjectId {
    let spell = put(state, caster, "Think Twice", Zone::Hand);
    for _ in 0..2 {
        let land = put(state, caster, "Island", Zone::Battlefield);
        priority(state, caster);
        engine::step(state, Action::ActivateManaAbility(land)).unwrap();
    }
    engine::step(state, Action::CastSpell(spell)).unwrap();
    engine::advance_until_decision(state);
    assert!(state.engine.event_history.iter().any(|event| matches!(event, CommittedEvent::SpellCast { spell: actual, controller } if *actual == spell && *controller == caster)));
    spell
}

fn payment(state: &mut GameState, payer: PlayerId) {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::ChooseEffectTargets { player, min_targets: 0, max_targets: 1, can_finish: true, .. } if player == payer)
        {
            effect::validate_pending_effect_choice(state).unwrap();
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("opponent did not receive optional discard");
}

#[test]
fn actual_3bb_printing_opponent_may_pay_or_decline_with_card_and_restore_after_source_leaves() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let (mut state, source) = paid_quandary(actor);
            let payer = actor.opponent();
            let card = put(&mut state, payer, "Lightning Bolt", Zone::Hand);
            cast_think_twice(&mut state, payer);
            assert_eq!(
                state.stack.last().unwrap().kind,
                StackItemKind::TriggeredAbility
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            payment(&mut state, payer);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for game in [&mut state, &mut restored] {
                effect::validate_pending_effect_choice(game).unwrap();
                if pay {
                    engine::step(game, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
                } else {
                    engine::step(game, Action::FinishEffectSelection).unwrap();
                }
                drain(game);
                assert_eq!(game.players[payer.index()].life, if pay { 20 } else { 15 });
                assert_eq!(game.players[actor.index()].life, 20);
                assert_eq!(
                    game.objects.get(card).zone,
                    if pay { Zone::Graveyard } else { Zone::Hand }
                );
                assert_eq!(game.objects.get(source).zone, Zone::Graveyard);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn actual_empty_hand_loses_five_and_controller_cast_does_not_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = paid_quandary(actor);
        cast_think_twice(&mut state, actor);
        assert!(state
            .stack
            .iter()
            .all(|item| item.kind != StackItemKind::TriggeredAbility));
        drain(&mut state);
        let payer = actor.opponent();
        assert!(state.players[payer.index()].hand.is_empty());
        cast_think_twice(&mut state, payer);
        assert!(state.players[payer.index()].hand.is_empty());
        drain(&mut state);
        assert_eq!(state.players[payer.index()].life, 15);
    }
}

#[test]
fn genuine_spell_copy_for_opponent_is_not_a_cast_and_creates_no_payment_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = paid_quandary(actor);
        let spell = cast_think_twice(&mut state, actor);
        let casts = state
            .engine
            .event_history
            .iter()
            .filter(|event| matches!(event, CommittedEvent::SpellCast { .. }))
            .count();
        let (copy, _) = engine::copy_spell_for(&mut state, spell, actor.opponent()).unwrap();
        assert!(state.objects.get(copy).spell_copy_origin.is_some());
        engine::advance_until_decision(&mut state);
        assert_eq!(
            state
                .engine
                .event_history
                .iter()
                .filter(|event| matches!(event, CommittedEvent::SpellCast { .. }))
                .count(),
            casts
        );
        assert!(state
            .stack
            .iter()
            .all(|item| item.kind != StackItemKind::TriggeredAbility));
        drain(&mut state);
        assert_eq!(state.players[actor.opponent().index()].life, 20);
    }
}

#[test]
fn unpaid_branch_payer_hand_generation_frames_and_answer_forgeries_are_rejected() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = paid_quandary(actor);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        cast_think_twice(&mut state, actor.opponent());
        payment(&mut state, actor.opponent());
        for mutation in 0..5 {
            let mut forged = state.clone();
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            let Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::DiscardPaymentV1 { choice },
                ..
            }) = pending.choice.as_mut()
            else {
                panic!("wrong payment purpose")
            };
            match mutation {
                0 => choice.player = actor,
                1 => choice.hand[0].expected_zone_change_count += 1,
                2 => {
                    *choice.unpaid = EffectOp::LoseLife {
                        player: crate::effect::PlayerRef::Opponent,
                        amount: 50,
                    }
                }
                3 => choice.path.push(0),
                _ => pending.frames.push(EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: crate::effect::PlayerRef::Controller,
                        count: 3,
                    },
                    path: vec![],
                }),
            }
            assert!(effect::validate_pending_effect_choice(&forged).is_err());
        }
        // A matching answer guard cannot authorize an injected trailing program.
        let mut forged = state.clone();
        let pending = forged.engine.pending_effect.as_mut().unwrap();
        let Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::DiscardPaymentV1 { choice },
            ..
        }) = pending.choice.take()
        else {
            panic!("wrong payment purpose")
        };
        let frame = EffectFrame::DiscardPaymentV1 {
            choice,
            selected: vec![],
        };
        pending.frames.push(EffectFrame::Program {
            op: EffectOp::DrawCards {
                player: crate::effect::PlayerRef::Controller,
                count: 3,
            },
            path: vec![],
        });
        pending.frames.push(frame.clone());
        pending.answered_choice_guard = Some(EffectAnsweredChoiceGuard::StandardSelection {
            frame: Box::new(frame),
        });
        assert!(effect::validate_pending_effect_choice(&forged).is_err());
    }
}

#[test]
fn opponent_projection_redacts_unpaid_private_hand_candidates() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let payer = actor.opponent();
        let (mut state, _) = paid_quandary(actor);
        put(&mut state, payer, "Lightning Bolt", Zone::Hand);
        cast_think_twice(&mut state, payer);
        payment(&mut state, payer);
        let surface = crate::surface_v2::HarnessSurfaceV2::new();
        let view = crate::rl::observe_v2(&state, &surface, actor, 0).unwrap();
        let bytes = serde_json::to_string(&view).unwrap();
        assert!(!bytes.contains("Lightning Bolt"));
        let Some(crate::rl::PendingEffectChoiceSemanticV4::Targets { legal_targets, .. }) = view
            .projection
            .engine_context
            .pending_effect
            .unwrap()
            .choice
        else {
            panic!("payment projection missing")
        };
        assert!(legal_targets.is_empty());
        let payer_view = crate::rl::observe_v2(&state, &surface, payer, 0).unwrap();
        let Some(crate::rl::PendingEffectChoiceSemanticV4::Targets { legal_targets, .. }) =
            payer_view
                .projection
                .engine_context
                .pending_effect
                .unwrap()
                .choice
        else {
            panic!("payer payment projection missing")
        };
        assert_eq!(legal_targets.len(), 1);
    }
}

#[test]
fn direct_resume_validates_genuine_discard_prompt_and_answer_before_mutation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let (mut state, source) = paid_quandary(actor);
            let payer = actor.opponent();
            let card = put(&mut state, payer, "Lightning Bolt", Zone::Hand);
            cast_think_twice(&mut state, payer);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            payment(&mut state, payer);
            let prompt = serde_json::to_vec(&state).unwrap();
            assert_eq!(
                effect::resume_resumable_resolution(&mut state).unwrap(),
                effect::ResumableProgress::Suspended
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), prompt);
            for answered in [false, true] {
                let mut checkpoint: GameState = serde_json::from_slice(&prompt).unwrap();
                if answered {
                    if pay {
                        engine::step(
                            &mut checkpoint,
                            Action::ChooseEffectTarget(Target::Object(card)),
                        )
                        .unwrap();
                    } else {
                        engine::step(&mut checkpoint, Action::FinishEffectSelection).unwrap();
                    }
                }
                let bytes = serde_json::to_vec(&checkpoint).unwrap();
                for mutation in 0..3 {
                    let mut forged: GameState = serde_json::from_slice(&bytes).unwrap();
                    let pending = forged.engine.pending_effect.as_mut().unwrap();
                    pending.choice = None;
                    pending.answered_choice_guard = None;
                    pending.frames = match mutation {
                        0 => vec![],
                        1 => vec![EffectFrame::Program {
                            op: EffectOp::DrawCards {
                                player: crate::effect::PlayerRef::Opponent,
                                count: 1,
                            },
                            path: vec![0],
                        }],
                        _ => vec![EffectFrame::Program {
                            op: EffectOp::LoseLife {
                                player: crate::effect::PlayerRef::Opponent,
                                amount: 50,
                            },
                            path: vec![0],
                        }],
                    };
                    let before = serde_json::to_vec(&forged).unwrap();
                    assert!(effect::validate_pending_effect_choice(&forged).is_err());
                    assert!(effect::resume_resumable_resolution(&mut forged).is_err());
                    assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
                }
                if answered {
                    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
                    let expected_item = checkpoint
                        .engine
                        .pending_effect
                        .as_ref()
                        .unwrap()
                        .resolving_item
                        .clone();
                    for game in [&mut checkpoint, &mut restored] {
                        assert_eq!(
                            effect::resume_resumable_resolution(game).unwrap(),
                            effect::ResumableProgress::Complete(Box::new(expected_item.clone()))
                        );
                        assert!(game.engine.pending_effect.is_none());
                        assert_eq!(game.players[payer.index()].life, if pay { 20 } else { 15 });
                        assert_eq!(
                            game.objects.get(card).zone,
                            if pay { Zone::Graveyard } else { Zone::Hand }
                        );
                        assert_eq!(game.players[actor.index()].life, 20);
                    }
                    assert_eq!(
                        serde_json::to_vec(&checkpoint).unwrap(),
                        serde_json::to_vec(&restored).unwrap()
                    );
                }
            }
        }
    }
}

#[test]
fn v55_paid_private_payment_and_answer_facts_restore_and_refuse_tampered_dto_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut opponent_views = Vec::new();
            for name in ["Lightning Bolt", "Llanowar Elves"] {
                let (mut state, source) = paid_quandary(actor);
                let payer = actor.opponent();
                let card = put(&mut state, payer, name, Zone::Hand);
                cast_think_twice(&mut state, payer);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                payment(&mut state, payer);
                let public = crate::quandary_public_v55::capture(&state, actor).unwrap();
                let own = crate::quandary_public_v55::capture(&state, payer).unwrap();
                let mut erased = state.clone();
                let pending = erased.engine.pending_effect.as_mut().unwrap();
                pending.choice = None;
                pending.frames.clear();
                pending.answered_choice_guard = None;
                assert!(crate::quandary_public_v55::capture(&erased, actor).is_err());
                assert!(public.current.is_empty());
                assert_eq!(public.stack[0].root.captured_controller, actor.into());
                assert!(public.stack[0].root.current_original_source.is_none());
                let crate::quandary_public_v55::StageV55::AwaitingPayment { payer_hand, .. } =
                    &public.stack[0].stage
                else {
                    panic!("public payment stage")
                };
                assert!(payer_hand.is_none());
                let crate::quandary_public_v55::StageV55::AwaitingPayment {
                    payer_hand: Some(hand),
                    minimum: 0,
                    maximum: 1,
                    ..
                } = &own.stack[0].stage
                else {
                    panic!("payer stage")
                };
                assert_eq!(hand.original_hand.len(), 1);
                assert_eq!(hand.original_hand[0].object, card.0);
                let bytes = serde_json::to_vec(&state).unwrap();
                let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    crate::quandary_public_v55::capture(&replay, actor).unwrap(),
                    public
                );
                let dto: crate::quandary_public_v55::FactsV55 =
                    serde_json::from_slice(&serde_json::to_vec(&own).unwrap()).unwrap();
                crate::quandary_public_v55::validate(&state, payer, &dto).unwrap();
                assert!(crate::quandary_public_v55::validate(&state, actor, &dto).is_err());
                let mut forged = public.clone();
                forged.stack[0].root.captured_controller = payer.into();
                let before = serde_json::to_vec(&state).unwrap();
                assert!(crate::quandary_public_v55::validate(&state, actor, &forged).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                opponent_views.push(public);
                for game in [&mut state, &mut replay] {
                    if pay {
                        engine::step(game, Action::ChooseEffectTarget(Target::Object(card)))
                            .unwrap();
                    } else {
                        engine::step(game, Action::FinishEffectSelection).unwrap();
                    }
                    let other = crate::quandary_public_v55::capture(game, actor).unwrap();
                    let own = crate::quandary_public_v55::capture(game, payer).unwrap();
                    assert!(matches!(
                        other.stack[0].stage,
                        crate::quandary_public_v55::StageV55::Answered {
                            payer_selected: None,
                            ..
                        }
                    ));
                    let crate::quandary_public_v55::StageV55::Answered {
                        payer_selected: Some(selected),
                        ..
                    } = &own.stack[0].stage
                    else {
                        panic!("private answered stage")
                    };
                    assert_eq!(selected.len(), usize::from(pay));
                    *game = serde_json::from_slice(&serde_json::to_vec(game).unwrap()).unwrap();
                    drain(game);
                    assert_eq!(game.players[payer.index()].life, if pay { 20 } else { 15 });
                    let complete = crate::quandary_public_v55::capture(game, actor).unwrap();
                    assert!(complete.stack.is_empty() && complete.held.is_empty());
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
            assert_eq!(opponent_views[0], opponent_views[1]);
        }
    }
}

#[test]
fn v55_genuine_spell_copy_is_not_a_cast_and_public_facts_keep_no_extra_root_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = paid_quandary(actor);
        let spell = cast_think_twice(&mut state, actor);
        engine::copy_spell_for(&mut state, spell, actor.opponent()).unwrap();
        engine::advance_until_decision(&mut state);
        for viewer in [actor, actor.opponent()] {
            let facts = crate::quandary_public_v55::capture(&state, viewer).unwrap();
            assert_eq!(facts.current.len(), 1);
            assert!(facts.held.is_empty() && facts.stack.is_empty());
        }
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        drain(&mut state);
        drain(&mut restored);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn v55_actual_live_discard_prompt_requires_empty_selected_and_exact_bound_hand_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let (mut state, _) = paid_quandary(actor);
            let payer = actor.opponent();
            let first = put(&mut state, payer, "Lightning Bolt", Zone::Hand);
            let second = put(&mut state, payer, "Llanowar Elves", Zone::Hand);
            cast_think_twice(&mut state, payer);
            payment(&mut state, payer);
            let original = serde_json::to_vec(&state).unwrap();
            for mutation in 0..5 {
                let mut forged: GameState = serde_json::from_slice(&original).unwrap();
                let Some(PendingEffectChoice::SelectTargets {
                    selected, legal, ..
                }) = forged
                    .engine
                    .pending_effect
                    .as_mut()
                    .unwrap()
                    .choice
                    .as_mut()
                else {
                    panic!("genuine payment prompt")
                };
                assert!(selected.is_empty());
                assert_eq!(legal.len(), 2);
                let unbound = effect::EffectTargetCandidate {
                    target: Target::Object(first),
                    expected_object: None,
                };
                match mutation {
                    0 => selected.push(unbound),
                    1 => legal.push(unbound),
                    2 => legal[0].expected_object = None,
                    3 => selected.push(legal.remove(0)),
                    _ => legal.push(legal[0].clone()),
                }
                let bytes = serde_json::to_vec(&forged).unwrap();
                assert!(effect::validate_pending_effect_choice(&forged).is_err());
                for viewer in [actor, payer] {
                    assert!(crate::quandary_public_v55::capture(&forged, viewer).is_err());
                }
                assert_eq!(serde_json::to_vec(&forged).unwrap(), bytes);
                for action in [
                    Action::ChooseEffectTarget(Target::Object(first)),
                    Action::FinishEffectSelection,
                ] {
                    let mut attempted: GameState = serde_json::from_slice(&bytes).unwrap();
                    assert!(engine::step(&mut attempted, action).is_err());
                    assert_eq!(serde_json::to_vec(&attempted).unwrap(), bytes);
                }
                assert!(effect::resume_resumable_resolution(&mut forged).is_err());
                assert_eq!(serde_json::to_vec(&forged).unwrap(), bytes);
            }
            assert_eq!(serde_json::to_vec(&state).unwrap(), original);
            let mut replay: GameState = serde_json::from_slice(&original).unwrap();
            for game in [&mut state, &mut replay] {
                if pay {
                    engine::step(game, Action::ChooseEffectTarget(Target::Object(first))).unwrap();
                } else {
                    engine::step(game, Action::FinishEffectSelection).unwrap();
                }
                assert!(game
                    .engine
                    .pending_effect
                    .as_ref()
                    .unwrap()
                    .choice
                    .is_none());
                effect::validate_pending_effect_choice(game).unwrap();
                crate::quandary_public_v55::capture(game, payer).unwrap();
                *game = serde_json::from_slice(&serde_json::to_vec(game).unwrap()).unwrap();
                drain(game);
                assert_eq!(game.players[payer.index()].life, if pay { 20 } else { 15 });
                assert_eq!(
                    game.objects.get(first).zone,
                    if pay { Zone::Graveyard } else { Zone::Hand }
                );
                assert_eq!(game.objects.get(second).zone, Zone::Hand);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
