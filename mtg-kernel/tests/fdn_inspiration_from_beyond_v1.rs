//! Future printed acceptance. Actual FDN admission is required, with no
//! synthetic definitions or registration bypass. Native execution is separate.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, CostComponent, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{
    self, EffectTargetSelectionPurpose, ObjectSelectionFilterV1, PendingEffectChoice,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(actor: PlayerId, mill_spells: bool) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut library = vec![forest; 40];
    if mill_spells {
        library[0] = card_id_by_name("Refute").unwrap();
        library[2] = card_id_by_name("Think Twice").unwrap();
    }
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &library,
        &library,
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_307,
        actor,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, actor: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("requires actual FDN card admission");
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: actor,
        controller: actor,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[actor.index()].hand.push(id),
        Zone::Graveyard => state.players[actor.index()].graveyard.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn until_selection(state: &mut GameState) -> Decision {
    for _ in 0..40 {
        match next(state) {
            d @ Decision::ChooseEffectTargets {
                player,
                selected_count,
                min_targets,
                max_targets,
                ..
            } => {
                let purpose = state
                    .engine
                    .pending_effect
                    .as_ref()
                    .unwrap()
                    .choice
                    .as_ref()
                    .unwrap();
                match purpose {
                    PendingEffectChoice::SelectTargets {
                        purpose: EffectTargetSelectionPurpose::SelectObjectsV1 { .. },
                        ..
                    } => return d,
                    PendingEffectChoice::SelectTargets {
                        purpose: EffectTargetSelectionPurpose::OrderMilledIntoGraveyard,
                        ..
                    } => {
                        assert_eq!(
                            player,
                            state.engine.pending_effect.as_ref().unwrap().ctx.controller
                        );
                        assert_eq!((min_targets, max_targets), (3, 3));
                        let chosen =
                            state.players[player.index()].library[usize::from(selected_count)];
                        // Preserve both private order-prompt and accepted partial
                        // answer snapshots before the public graveyard move.
                        *state = restore(state);
                        engine::step(state, Action::ChooseEffectTarget(Target::Object(chosen)))
                            .unwrap();
                        *state = restore(state);
                    }
                    _ => panic!("unexpected typed selection stage"),
                }
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("resolution choice: {other:?}"),
        }
    }
    panic!("selection bound");
}

fn settle(state: &mut GameState) {
    for _ in 0..40 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_effect.is_none() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("settle {other:?}"),
        }
    }
    panic!("settle bound");
}

fn cast(state: &mut GameState, actor: PlayerId, source: ObjectId, flashback: bool) {
    state.players[actor.index()].mana_pool = [0; 6];
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] =
        if flashback { 2 } else { 1 };
    state.players[actor.index()].mana_pool[5] = if flashback { 5 } else { 2 };
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    // No announcement targets, even with an empty graveyard. The resolving
    // card remains on the stack and cannot choose itself after the mill.
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.last().unwrap().source, source);
    assert!(state.stack.last().unwrap().targets.is_empty());
}

#[test]
fn printed_costs_and_no_target_recipe_require_actual_admission() {
    let def = &CARD_DEFS[card_id_by_name("Inspiration from Beyond").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.target_spec, TargetSpec::None);
    assert_eq!(def.cost.generic, 2);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert!(
        matches!(def.flashback.unwrap().cost, [CostComponent::Mana(cost)]
        if cost.generic == 5 && cost.pips == [Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)])
    );
}

#[test]
fn actual_cast_mill_three_mandatory_non_target_return_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, true);
        let source = put(&mut state, actor, "Inspiration from Beyond", Zone::Hand);
        let prior = put(&mut state, actor, "Think Twice", Zone::Graveyard);
        let opposing = put(&mut state, actor.opponent(), "Refute", Zone::Graveyard);
        let milled = state.players[actor.index()].library[..3].to_vec();
        cast(&mut state, actor, source, false);
        state = restore(&state);
        let decision = until_selection(&mut state);
        state.engine.event_log.clear();
        effect::validate_pending_effect_choice(&state).unwrap();
        assert!(
            matches!(decision, Decision::ChooseEffectTargets { player, min_targets: 1, max_targets: 1, can_finish: false, ref legal_targets, .. }
            if player == actor && legal_targets.contains(&Target::Object(prior))
                && legal_targets.contains(&Target::Object(milled[0])) && legal_targets.contains(&Target::Object(milled[2]))
                && !legal_targets.contains(&Target::Object(milled[1])) && !legal_targets.contains(&Target::Object(source))
                && !legal_targets.contains(&Target::Object(opposing)))
        );
        assert_eq!(state.players[actor.index()].library.len(), 37);
        let public = mtg_kernel::policy_surface_v5::PolicyDecisionV5::Surface(
            mtg_kernel::surface_v2::SurfaceDecision::Decision(decision.clone()),
        );
        assert_eq!(
            mtg_kernel::rl::legal_action_candidates_v5(&public, &state)
                .unwrap()
                .len(),
            3
        );
        assert!(milled
            .iter()
            .all(|&id| state.objects.get(id).zone == Zone::Graveyard));
        state = restore(&state);
        let before = state.clone();
        assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
        assert_eq!(state, before);
        assert!(engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(opposing))
        )
        .is_err());
        assert_eq!(state, before);
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(milled[0])),
        )
        .unwrap();
        state = restore(&state);
        settle(&mut state);
        assert_eq!(state.objects.get(milled[0]).zone, Zone::Hand);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(opposing).zone, Zone::Graveyard);
    }
}

#[test]
fn empty_legal_return_still_mills_and_flashback_exiles_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            let mut state = ready(actor, false);
            let source = put(
                &mut state,
                actor,
                "Inspiration from Beyond",
                if flashback {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                },
            );
            cast(&mut state, actor, source, flashback);
            let decision = until_selection(&mut state);
            assert!(
                matches!(decision, Decision::ChooseEffectTargets { min_targets: 0, max_targets: 0, ref legal_targets, can_finish: true, .. } if legal_targets.is_empty())
            );
            let public = mtg_kernel::policy_surface_v5::PolicyDecisionV5::Surface(
                mtg_kernel::surface_v2::SurfaceDecision::Decision(decision),
            );
            assert_eq!(
                mtg_kernel::rl::legal_action_candidates_v5(&public, &state)
                    .unwrap()
                    .len(),
                1
            );
            state = restore(&state);
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
            settle(&mut state);
            assert_eq!(state.players[actor.index()].library.len(), 37);
            assert_eq!(
                state.objects.get(source).zone,
                if flashback {
                    Zone::Exile
                } else {
                    Zone::Graveyard
                }
            );
            assert!(state.players[actor.index()].hand.is_empty());
        }
    }
}

#[test]
fn selection_restore_refuses_coherent_rule_candidate_and_incarnation_forgeries_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, true);
        let source = put(&mut state, actor, "Inspiration from Beyond", Zone::Hand);
        cast(&mut state, actor, source, false);
        until_selection(&mut state);
        for forgery in 0..11 {
            let mut altered = restore(&state);
            if forgery == 10 {
                altered
                    .engine
                    .event_log
                    .push(altered.engine.event_history.last().unwrap().clone());
            } else if forgery == 9 {
                altered
                    .engine
                    .event_history
                    .push(altered.engine.event_history.last().unwrap().clone());
            } else if forgery == 8 {
                altered.engine.pending_effect = None;
            } else if forgery == 7 {
                assert!(matches!(
                    altered.engine.event_history.pop(),
                    Some(event::CommittedEvent::InspirationResolutionBoundaryV1 { .. })
                ));
            } else if forgery == 6 {
                let pending = altered.engine.pending_effect.as_mut().unwrap();
                pending.choice = None;
                pending.answered_choice_guard = None;
                pending.frames = vec![effect::EffectFrame::Program {
                    op: (CARD_DEFS[altered.objects.get(source).card_def as usize].spell_effect)()
                        .unwrap(),
                    path: Vec::new(),
                }];
            } else if forgery == 5 {
                let pending = altered.engine.pending_effect.as_mut().unwrap();
                pending.choice = None;
                pending.frames.clear();
                pending.answered_choice_guard = None;
            } else if forgery == 4 {
                let chosen = altered.players[actor.index()].graveyard[0];
                event::propose_and_commit(
                    &mut altered,
                    ProposedEvent::zone_change(chosen, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut altered,
                    ProposedEvent::zone_change(chosen, Zone::Graveyard),
                );
            } else {
                let Some(PendingEffectChoice::SelectTargets {
                    player,
                    min_targets,
                    max_targets,
                    purpose: EffectTargetSelectionPurpose::SelectObjectsV1 { rule, .. },
                    ..
                }) = altered
                    .engine
                    .pending_effect
                    .as_mut()
                    .unwrap()
                    .choice
                    .as_mut()
                else {
                    panic!("selection contract");
                };
                match forgery {
                    0 => {
                        rule.filter = ObjectSelectionFilterV1::Creature;
                    }
                    1 => {
                        rule.min = 0;
                        *min_targets = 0;
                    }
                    2 => {
                        rule.max = 2;
                        *max_targets = 2;
                    }
                    _ => {
                        *player = actor.opponent();
                        rule.player = effect::PlayerRef::Opponent;
                    }
                }
            }
            let before = altered.clone();
            assert!(effect::validate_pending_effect_choice(&altered).is_err());
            assert_eq!(altered, before);
            let candidate = state.players[actor.index()].graveyard[0];
            assert!(engine::step(
                &mut altered,
                Action::ChooseEffectTarget(Target::Object(candidate))
            )
            .is_err());
            assert_eq!(altered, before);
            assert!(engine::step(&mut altered, Action::Pass).is_err());
            assert_eq!(altered, before);
        }
    }
}

#[test]
fn empty_library_mill_has_authenticated_empty_return_and_cannot_restart_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[],
            &[],
            |_| "Forest".into(),
            110_308,
            actor,
        );
        state.step = Step::Main1;
        let source = put(&mut state, actor, "Inspiration from Beyond", Zone::Hand);
        cast(&mut state, actor, source, false);
        assert!(matches!(
            until_selection(&mut state),
            Decision::ChooseEffectTargets {
                min_targets: 0,
                max_targets: 0,
                can_finish: true,
                ..
            }
        ));
        state = restore(&state);
        let mut erased = state.clone();
        let pending = erased.engine.pending_effect.as_mut().unwrap();
        pending.choice = None;
        pending.frames.clear();
        pending.answered_choice_guard = None;
        let before = erased.clone();
        assert!(engine::step(&mut erased, Action::Pass).is_err());
        assert_eq!(erased, before);
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        state = restore(&state);
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert!(state.players[actor.index()].hand.is_empty());
    }
}

#[test]
fn same_physical_card_can_flash_back_after_its_closed_normal_resolution_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, true);
        let source = put(&mut state, actor, "Inspiration from Beyond", Zone::Hand);
        let first = state.players[actor.index()].library[0];
        let second = state.players[actor.index()].library[2];
        cast(&mut state, actor, source, false);
        let original_stack = state.stack.last().unwrap().v4.stack_item_id;
        until_selection(&mut state);
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(first)),
        )
        .unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        cast(&mut state, actor, source, true);
        assert_ne!(state.stack.last().unwrap().v4.stack_item_id, original_stack);
        state = restore(&state);
        assert!(
            matches!(until_selection(&mut state), Decision::ChooseEffectTargets { ref legal_targets, can_finish: false, .. }
            if legal_targets == &[Target::Object(second)])
        );
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(second)),
        )
        .unwrap();
        state = restore(&state);
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Exile);
        assert_eq!(state.objects.get(first).zone, Zone::Hand);
        assert_eq!(state.objects.get(second).zone, Zone::Hand);
        assert_eq!(state.players[actor.index()].library.len(), 34);
    }
}

#[test]
fn ordinary_counter_response_prevents_mill_and_flashback_still_exiles_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            let mut state = ready(actor, true);
            let source = put(
                &mut state,
                actor,
                "Inspiration from Beyond",
                if flashback {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                },
            );
            cast(&mut state, actor, source, flashback);
            engine::step(&mut state, Action::Pass).unwrap();
            let counter = put(
                &mut state,
                actor.opponent(),
                "An Offer You Can't Refuse",
                Zone::Hand,
            );
            state.players[actor.opponent().index()].mana_pool[ManaColor::U.pool_index()] = 1;
            state.players[actor.opponent().index()].mana_pool[5] = 0;
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. } if player == actor.opponent() && castable_spells.contains(&counter))
            );
            engine::step(&mut state, Action::CastSpell(counter)).unwrap();
            let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
                panic!("counter target");
            };
            assert_eq!(legal_targets.len(), 1);
            engine::step(&mut state, Action::ChooseTarget(legal_targets[0])).unwrap();
            state = restore(&state);
            settle(&mut state);
            assert_eq!(state.players[actor.index()].library.len(), 40);
            assert_eq!(
                state.objects.get(source).zone,
                if flashback {
                    Zone::Exile
                } else {
                    Zone::Graveyard
                }
            );
        }
    }
}

#[test]
fn real_owned_prompt_cannot_resume_or_restart_through_public_effect_entry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, true);
        let source = put(&mut state, actor, "Inspiration from Beyond", Zone::Hand);
        cast(&mut state, actor, source, false);
        let _ = until_selection(&mut state);
        let pending = state.engine.pending_effect.as_ref().unwrap().clone();
        let root = (CARD_DEFS[state.objects.get(source).card_def as usize].spell_effect)().unwrap();
        for remove_live_row in [false, true] {
            let mut forged = restore(&state);
            if remove_live_row {
                forged
                    .stack
                    .retain(|row| row.v4.stack_item_id != pending.resolving_item.v4.stack_item_id);
            }
            let before = forged.clone();
            assert!(effect::resume_resumable_resolution(&mut forged).is_err());
            assert_eq!(forged, before);

            // Preserve the actual publication/answer journals, but try to
            // restart from the complete genuine row and execution context.
            forged.engine.pending_effect = None;
            let before = forged.clone();
            let error = effect::begin_resumable_resolution(
                &root,
                &pending.ctx,
                pending.resolving_item.clone(),
                &mut forged,
            )
            .unwrap_err();
            assert!(error.contains("genuine private pre-pop owner"), "{error}");
            assert_eq!(forged, before);
        }
    }
}

fn reject_closed_or_restarted_readonly(mut bad: GameState) {
    let mut restored = restore(&bad);
    for state in [&mut bad, &mut restored] {
        let before = serde_json::to_vec(state).unwrap();
        assert!(effect::validate_pending_effect_choice(state).is_err());
        assert!(engine::step(state, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(state).unwrap(), before);
        let mut expected = restore(state);
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::Halted { .. }
        ));
        expected.engine.halted = state.engine.halted;
        assert_eq!(
            serde_json::to_vec(state).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
    }
    assert_eq!(
        serde_json::to_vec(&bad).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}
#[test]
fn original_journal_cannot_close_or_restart_the_real_mandatory_return_both_seats() {
    use event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, true);
        let source = put(&mut state, actor, "Inspiration from Beyond", Zone::Hand);
        cast(&mut state, actor, source, false);
        until_selection(&mut state);
        let original = state.engine.event_history.clone();
        let item = state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .resolving_item
            .clone();
        assert!(item.v4.inspiration_started_v1.0.is_some());
        let receipt = state
            .engine
            .event_history
            .iter()
            .rev()
            .find_map(|e| match e {
                CommittedEvent::InspirationResolutionBoundaryV1 { receipt } => {
                    Some(receipt.clone())
                }
                _ => None,
            })
            .unwrap();
        let mut forged = state.clone();
        forged.engine.pending_effect = None;
        forged
            .stack
            .retain(|row| row.v4.stack_item_id != item.v4.stack_item_id);
        let mut closed = receipt.clone();
        closed.previous_history_index = Some(receipt.history_index);
        closed.history_index = forged.engine.event_history.len() as u32;
        closed.ordinal += 1;
        closed.pending = None;
        forged
            .engine
            .event_history
            .push(CommittedEvent::InspirationResolutionBoundaryV1 { receipt: closed });
        assert_eq!(forged.engine.event_history[..original.len()], original);
        reject_closed_or_restarted_readonly(forged);
        let mut erased = state.clone();
        erased.engine.pending_effect = None;
        for event in &mut erased.engine.event_history {
            if matches!(
                event,
                CommittedEvent::InspirationResolutionBoundaryV1 { .. }
                    | CommittedEvent::InspirationResolutionInputV1 { .. }
                    | CommittedEvent::InspirationResolutionOperationV1 { .. }
            ) {
                *event = CommittedEvent::Tap { object: source };
            }
        }
        assert!(erased.stack.iter().any(|row| row == &item));
        reject_closed_or_restarted_readonly(erased);
        let mut unmarked = state.clone();
        unmarked
            .stack
            .iter_mut()
            .find(|row| row.v4.stack_item_id == item.v4.stack_item_id)
            .unwrap()
            .v4
            .inspiration_started_v1 = Default::default();
        reject_closed_or_restarted_readonly(unmarked);
    }
}
#[test]
fn actual_return_and_final_spell_departure_are_independent_required_occurrences_both_seats() {
    use event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            let mut state = ready(actor, true);
            let source = put(
                &mut state,
                actor,
                "Inspiration from Beyond",
                if flashback {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                },
            );
            let chosen = state.players[actor.index()].library[0];
            cast(&mut state, actor, source, flashback);
            until_selection(&mut state);
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(chosen)),
            )
            .unwrap();
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                settle(branch);
                effect::validate_pending_effect_choice(branch).unwrap();
                assert_eq!(branch.objects.get(chosen).zone, Zone::Hand);
                assert_eq!(
                    branch.objects.get(source).zone,
                    if flashback {
                        Zone::Exile
                    } else {
                        Zone::Graveyard
                    }
                );
                assert!(branch
                    .engine
                    .event_history
                    .iter()
                    .any(|e| matches!(e, CommittedEvent::InspirationEngineDepartureV1 { .. })));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            let departure_at = state
                .engine
                .event_history
                .iter()
                .position(|e| matches!(e, CommittedEvent::InspirationEngineDepartureV1 { .. }))
                .unwrap();
            let CommittedEvent::InspirationEngineDepartureV1 { departure } =
                &state.engine.event_history[departure_at]
            else {
                unreachable!()
            };
            let movement = departure.movement.unwrap() as usize;
            let return_at = state.engine.event_history.iter().position(|e|matches!(e,
                CommittedEvent::ZoneChange {object,from:Zone::Graveyard,to:Zone::Hand,..} if *object == chosen)).unwrap();
            for at in [departure_at, movement, return_at] {
                let mut bad = state.clone();
                bad.engine.event_history[at] = CommittedEvent::Tap { object: source };
                reject_closed_or_restarted_readonly(bad);
            }
            let mut replayed = state.clone();
            let first = replayed
                .engine
                .event_history
                .iter()
                .find_map(|e| match e {
                    CommittedEvent::InspirationResolutionBoundaryV1 { receipt }
                        if receipt.ordinal == 0 =>
                    {
                        receipt.pending.as_ref().map(|p| p.resolving_item.clone())
                    }
                    _ => None,
                })
                .unwrap();
            replayed.stack.push(first);
            reject_closed_or_restarted_readonly(replayed);
        }
    }
}

#[test]
fn accepted_return_target_and_finish_input_must_match_the_actual_selection_transition_both_seats() {
    use event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, true);
        let source = put(&mut state, actor, "Inspiration from Beyond", Zone::Hand);
        let chosen = state.players[actor.index()].library[0];
        let other = state.players[actor.index()].library[2];
        cast(&mut state, actor, source, false);
        until_selection(&mut state);
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(chosen)),
        )
        .unwrap();
        effect::validate_pending_effect_choice(&state).unwrap();
        let input_at = state.engine.event_history.iter().rposition(|event|matches!(event,
            CommittedEvent::InspirationResolutionInputV1 { input } if matches!(input.kind, effect::InspirationInputKindV1::Target(_)))).unwrap();
        for kind in [
            effect::InspirationInputKindV1::Target(Target::Object(other)),
            effect::InspirationInputKindV1::Finish,
        ] {
            let mut altered = state.clone();
            let CommittedEvent::InspirationResolutionInputV1 { input } =
                &mut altered.engine.event_history[input_at]
            else {
                unreachable!()
            };
            input.kind = kind;
            reject_closed_or_restarted_readonly(altered);
        }
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            settle(branch);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
