//! Focused rules, continuation-integrity, provenance, and composition coverage
//! for Deem Inferior. The bounded rules baseline is XMage commit
//! `0723fc0c2be922af47b0ef0539f28114cc23b998`.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, DynamicCountDef, GenericCostReductionDef, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::effect::{
    EffectAnsweredChoiceGuard, EffectFrame, EffectObjectBinding, EffectOp,
    EffectOptionChoicePurpose,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision, UnsupportedMechanic};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{legal_action_candidates_v1, observe_v2, ActionSemanticV1, TargetRefV1};
use mtg_kernel::state::{
    Counters, GameObject, GameState, LibraryKnowledgeEntry, StackTargetContractV4, Step, Target,
    Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};
use mtg_kernel::trigger;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn put_object(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        Zone::Library => state.players[owner.index()].library.push(object),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Command => state.command.push(object),
        Zone::Stack => panic!("casts own stack insertion"),
    }
    object
}

fn ready_deem(p1_library: &[&str]) -> (GameState, ObjectId) {
    let p0 = [card_id("Island")];
    let p1 = p1_library
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let mut state = GameState::new_from_libraries(&p0, &p1, card_name, 0x4445_454d);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 3;
    let deem = put_object(&mut state, PlayerId::P0, "Deem Inferior", Zone::Hand);
    (state, deem)
}

fn cast_and_target(state: &mut GameState, deem: ObjectId, target: ObjectId) {
    engine::step(state, Action::CastSpell(deem)).unwrap();
    let decision = engine::advance_until_decision(state);
    assert!(matches!(
        decision,
        Decision::ChooseTargets { spell, .. } if spell == deem
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    let priority = engine::advance_until_decision(state);
    assert!(matches!(priority, Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
}

fn advance_to_deem_choice(state: &mut GameState, deem: ObjectId) -> Decision {
    for _ in 0..8 {
        let decision = engine::advance_until_decision(state);
        match decision {
            choice @ Decision::ChooseEffectOption {
                source,
                option_count: 2,
                ..
            } if source == deem => return choice,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before Deem choice: {other:?}"),
        }
    }
    panic!("Deem did not reach its owner placement choice")
}

fn pass_until_spell_finishes(state: &mut GameState, spell: ObjectId) {
    for _ in 0..12 {
        if state.objects.get(spell).zone != Zone::Stack {
            return;
        }
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving spell: {other:?}"),
        }
    }
    panic!("spell did not finish")
}

fn assert_tampered_stack_target_halts(state: &mut GameState, spell: ObjectId) {
    let projection_error = observe_v2(state, &HarnessSurfaceV2::new(), PlayerId::P0, 0)
        .expect_err("malformed historical target metadata must not reach RL projection");
    assert!(projection_error
        .to_string()
        .contains("stack target contract is structurally malformed"));
    let stack_before = state.stack.clone();
    engine::step(state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == spell
    ));
    assert_eq!(state.stack, stack_before);
}

fn spell_is_offered(state: &mut GameState, spell: ObjectId) -> bool {
    matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)
    )
}

#[test]
fn generated_definition_cost_reducer_target_and_effect_are_exact() {
    let def = &CARD_DEFS[card_id("Deem Inferior") as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.cost.generic, 3);
    assert_eq!(
        def.cost.pips,
        &[mtg_kernel::mana::Pip::Colored(ManaColor::U)]
    );
    assert_eq!(def.target_spec, TargetSpec::NonlandPermanent);
    assert_eq!(
        def.generic_cost_reduction,
        Some(GenericCostReductionDef {
            generic_per_count: 1,
            count: DynamicCountDef::ControllerDrawsThisTurn,
        })
    );
    assert_eq!(
        (def.spell_effect)(),
        Some(EffectOp::PutObjectInOwnersLibrarySecondOrBottom {
            object: mtg_kernel::effect::ObjectRef::Target(0),
        })
    );
}

#[test]
fn target_pool_is_both_battlefields_all_nonlands_and_no_lands() {
    let (mut state, deem) = ready_deem(&[]);
    let own_artifact = put_object(
        &mut state,
        PlayerId::P0,
        "Experimental Synthesizer",
        Zone::Battlefield,
    );
    let their_creature = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    let own_land = put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let their_artifact_land =
        put_object(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    engine::step(&mut state, Action::CastSpell(deem)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = engine::advance_until_decision(&mut state)
    else {
        panic!("expected Deem target choice")
    };
    assert!(legal_targets.contains(&Target::Object(own_artifact)));
    assert!(legal_targets.contains(&Target::Object(their_creature)));
    assert!(!legal_targets.contains(&Target::Object(own_land)));
    assert!(!legal_targets.contains(&Target::Object(their_artifact_land)));
}

#[test]
fn draw_count_reduces_only_generic_cost_from_zero_through_saturation() {
    for draws in 0..=5 {
        let (mut state, deem) = ready_deem(&[]);
        put_object(
            &mut state,
            PlayerId::P1,
            "Cryptic Serpent",
            Zone::Battlefield,
        );
        state.players[0].draws_this_turn = draws;
        let needed = 3_u8.saturating_sub(draws as u8);
        state.players[0].mana_pool[ManaColor::C.pool_index()] = needed;
        assert!(spell_is_offered(&mut state, deem));

        if needed > 0 {
            let mut short = state.clone();
            short.players[0].mana_pool[ManaColor::C.pool_index()] -= 1;
            assert!(!spell_is_offered(&mut short, deem));
        }
        let mut no_blue = state.clone();
        no_blue.players[0].mana_pool[ManaColor::U.pool_index()] = 0;
        assert!(!spell_is_offered(&mut no_blue, deem));

        state.players[1].draws_this_turn = 99;
        assert!(spell_is_offered(&mut state, deem));
    }
}

#[test]
fn real_draw_events_failed_draw_opponent_draw_and_untap_reset_drive_the_reducer() {
    let (mut successful, deem) = ready_deem(&["Mountain"]);
    let target = put_object(
        &mut successful,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    successful.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    event::propose_and_commit(&mut successful, ProposedEvent::draw(PlayerId::P0));
    assert_eq!(successful.players[0].draws_this_turn, 1);
    assert!(spell_is_offered(&mut successful, deem));
    assert!(successful.players[1].battlefield.contains(&target));

    successful.step = Step::Cleanup;
    let _ = engine::advance_until_decision(&mut successful);
    assert_eq!(successful.players[0].draws_this_turn, 0);
    successful.step = Step::Main1;
    successful.active_player = PlayerId::P0;
    successful.priority_player = PlayerId::P0;
    successful.engine.priority_passes = [false, false];
    successful.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    successful.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    assert!(engine::step(&mut successful, Action::CastSpell(deem)).is_err());

    let mut failed = GameState::new_from_libraries(&[], &[], card_name, 0x4445_454e_0002);
    failed.step = Step::Main1;
    failed.active_player = PlayerId::P0;
    failed.priority_player = PlayerId::P0;
    failed.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    failed.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    let failed_deem = put_object(&mut failed, PlayerId::P0, "Deem Inferior", Zone::Hand);
    put_object(
        &mut failed,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    event::propose_and_commit(&mut failed, ProposedEvent::draw(PlayerId::P0));
    assert_eq!(failed.players[0].draws_this_turn, 0);
    assert!(failed.players[0].drew_from_empty);
    assert!(engine::step(&mut failed, Action::CastSpell(failed_deem)).is_err());

    let (mut opponent, opponent_deem) = ready_deem(&["Mountain"]);
    put_object(
        &mut opponent,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    opponent.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    event::propose_and_commit(&mut opponent, ProposedEvent::draw(PlayerId::P1));
    assert_eq!(opponent.players[0].draws_this_turn, 0);
    assert_eq!(opponent.players[1].draws_this_turn, 1);
    assert!(engine::step(&mut opponent, Action::CastSpell(opponent_deem)).is_err());
}

#[test]
fn live_choice_action_order_and_snapshot_restore_are_deterministic() {
    let (mut state, deem) = ready_deem(&["Mountain", "Island", "Forest"]);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    cast_and_target(&mut state, deem, target);
    let decision = advance_to_deem_choice(&mut state, deem);
    let actions =
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), &state).unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(
        actions
            .iter()
            .map(|candidate| match candidate.record.semantic {
                ActionSemanticV1::ChooseEffectOption {
                    option_index,
                    option_count: 2,
                    ..
                } => option_index,
                ref other => panic!("unexpected Deem option action: {other:?}"),
            })
            .collect::<Vec<_>>(),
        vec![0, 1],
        "printed second-from-top then bottom order is stable"
    );

    let snapshot = state.snapshot();
    engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    let expected = state.clone();
    state.restore(&snapshot);
    assert_eq!(advance_to_deem_choice(&mut state, deem), decision);
    engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state, expected);
}

#[test]
fn owner_not_controller_chooses_and_both_observers_learn_exact_position() {
    for (library, option, expected_position) in [
        (vec![], 0_u16, 0_usize),
        (vec!["Mountain"], 0, 1),
        (vec!["Mountain", "Island", "Forest"], 0, 1),
        (vec!["Mountain", "Island", "Forest"], 1, 3),
    ] {
        let (mut state, deem) = ready_deem(&library);
        let target = put_object(
            &mut state,
            PlayerId::P1,
            "Cryptic Serpent",
            Zone::Battlefield,
        );
        state.players[1].battlefield.retain(|id| *id != target);
        state.players[0].battlefield.push(target);
        state.objects.get_mut(target).controller = PlayerId::P0;
        cast_and_target(&mut state, deem, target);
        assert!(matches!(
            advance_to_deem_choice(&mut state, deem),
            Decision::ChooseEffectOption {
                player: PlayerId::P1,
                ..
            }
        ));
        let history_start = state.engine.event_history.len();
        engine::step(&mut state, Action::ChooseEffectOption(option)).unwrap();
        let _ = engine::advance_until_decision(&mut state);

        assert_eq!(state.players[1].library[expected_position], target);
        assert!(!state.players[0].battlefield.contains(&target));
        assert_eq!(state.objects.get(target).controller, PlayerId::P1);
        for observer in [PlayerId::P0, PlayerId::P1] {
            let known = state.known_library_cards(observer, PlayerId::P1);
            assert_eq!(
                known.len(),
                1,
                "public insertion must not reveal unknown neighbors"
            );
            assert_eq!(known[0].position as usize, expected_position);
            assert_eq!(known[0].object, target);
            assert_eq!(
                known[0].zone_change_count,
                state.objects.get(target).zone_change_count
            );
        }
        assert!(matches!(
            &state.engine.event_history[history_start],
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Library,
                controller_before: PlayerId::P0,
            } if *object == target
        ));
    }
}

#[test]
fn public_second_position_preserves_sparse_prior_facts_without_neighbor_leaks() {
    let (mut state, deem) = ready_deem(&["Mountain", "Island", "Forest"]);
    let original = state.players[1].library.clone();
    state.library_knowledge[PlayerId::P0.index()][PlayerId::P1.index()] =
        vec![LibraryKnowledgeEntry {
            position: 0,
            object: original[0],
            zone_change_count: 0,
        }];
    state.library_knowledge[PlayerId::P1.index()][PlayerId::P1.index()] =
        vec![LibraryKnowledgeEntry {
            position: 2,
            object: original[2],
            zone_change_count: 0,
        }];
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    cast_and_target(&mut state, deem, target);
    advance_to_deem_choice(&mut state, deem);
    engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
    let _ = engine::advance_until_decision(&mut state);

    assert_eq!(
        state.players[1].library,
        vec![original[0], target, original[1], original[2]]
    );
    assert_eq!(
        state
            .known_library_cards(PlayerId::P0, PlayerId::P1)
            .iter()
            .map(|entry| (entry.position, entry.object))
            .collect::<Vec<_>>(),
        vec![(0, original[0]), (1, target)]
    );
    assert_eq!(
        state
            .known_library_cards(PlayerId::P1, PlayerId::P1)
            .iter()
            .map(|entry| (entry.position, entry.object))
            .collect::<Vec<_>>(),
        vec![(1, target), (3, original[2])]
    );
}

#[test]
fn stale_target_fizzles_but_historical_rl_target_survives_leave_reenter() {
    let (mut state, deem) = ready_deem(&[]);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    cast_and_target(&mut state, deem, target);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(target, Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(target, Zone::Battlefield),
    );
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let TargetRefV1::Object { object } = &observation.projection.stack[0].targets[0] else {
        panic!("object target")
    };
    assert_eq!(object.zone, Zone::Battlefield);
    assert_eq!(
        object.zone_change_count, 0,
        "RL projects cast-time provenance"
    );
    pass_until_spell_finishes(&mut state, deem);
    assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(deem).zone, Zone::Graveyard);
}

#[test]
fn token_target_moves_then_ceases_and_synthesizer_leave_trigger_composes() {
    let (mut token_state, token_deem) = ready_deem(&[]);
    let token = put_object(
        &mut token_state,
        PlayerId::P1,
        "Blood Token",
        Zone::Battlefield,
    );
    cast_and_target(&mut token_state, token_deem, token);
    advance_to_deem_choice(&mut token_state, token_deem);
    engine::step(&mut token_state, Action::ChooseEffectOption(1)).unwrap();
    let _ = engine::advance_until_decision(&mut token_state);
    assert!(!token_state.players[1].library.contains(&token));
    assert!(!token_state.players[1].battlefield.contains(&token));
    assert!(token_state
        .engine
        .event_history
        .iter()
        .any(|event| matches!(
            event,
            CommittedEvent::ZoneChange { object, to: Zone::Library, .. } if *object == token
        )));

    let (mut synth_state, synth_deem) = ready_deem(&["Mountain"]);
    let synth = put_object(
        &mut synth_state,
        PlayerId::P1,
        "Experimental Synthesizer",
        Zone::Battlefield,
    );
    cast_and_target(&mut synth_state, synth_deem, synth);
    advance_to_deem_choice(&mut synth_state, synth_deem);
    engine::step(&mut synth_state, Action::ChooseEffectOption(0)).unwrap();
    let _ = engine::advance_until_decision(&mut synth_state);
    assert!(synth_state.stack.iter().any(|item| {
        item.source == synth && item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility
    }));
}

fn pending_deem_with_two_targets() -> (GameState, ObjectId, ObjectId, ObjectId) {
    let (mut state, deem) = ready_deem(&[]);
    let a = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    let b = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    cast_and_target(&mut state, deem, a);
    advance_to_deem_choice(&mut state, deem);
    (state, deem, a, b)
}

#[test]
fn pending_and_answered_choice_tampering_fails_closed_without_moving_a_permanent() {
    let (mut active_redirect, _, a, b) = pending_deem_with_two_targets();
    let b_binding = EffectObjectBinding {
        object: b,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: active_redirect.objects.get(b).zone_change_count,
    };
    let mtg_kernel::effect::PendingEffectChoice::ChooseOption {
        options, purpose, ..
    } = active_redirect
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        panic!("option choice")
    };
    let EffectOptionChoicePurpose::OwnerLibrarySecondOrBottom { object, .. } = purpose else {
        panic!("typed owner-library purpose")
    };
    *object = b_binding;
    for option in options {
        let EffectOp::PutBoundObjectInOwnersLibrary { object, .. } = option else {
            panic!("bound owner-library option")
        };
        *object = b_binding;
    }
    let redirected_before_step = active_redirect.clone();
    assert!(engine::step(&mut active_redirect, Action::ChooseEffectOption(0)).is_err());
    assert_eq!(active_redirect, redirected_before_step);
    assert!(active_redirect.players[1].battlefield.contains(&a));
    assert!(active_redirect.players[1].battlefield.contains(&b));

    let (mut active_injection, _, a, _) = pending_deem_with_two_targets();
    let injected = EffectFrame::Program {
        op: EffectOp::GainLife {
            player: mtg_kernel::effect::PlayerRef::Controller,
            amount: 99,
        },
        path: vec![99],
    };
    let continuation = active_injection.engine.pending_effect.as_mut().unwrap();
    continuation.frames.push(injected.clone());
    let mtg_kernel::effect::PendingEffectChoice::ChooseOption { path, purpose, .. } =
        continuation.choice.as_mut().unwrap()
    else {
        panic!("option choice")
    };
    *path = vec![7];
    let EffectOptionChoicePurpose::OwnerLibrarySecondOrBottom {
        canonical_path,
        expected_remaining_frames,
        ..
    } = purpose
    else {
        panic!("typed owner-library purpose")
    };
    *canonical_path = vec![7];
    expected_remaining_frames.push(injected.clone());
    let injected_before_step = active_injection.clone();
    assert!(engine::step(&mut active_injection, Action::ChooseEffectOption(0)).is_err());
    assert_eq!(active_injection, injected_before_step);
    assert!(active_injection.players[1].battlefield.contains(&a));

    let (mut pending, _, _, _) = pending_deem_with_two_targets();
    let mtg_kernel::effect::PendingEffectChoice::ChooseOption { options, .. } = pending
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        panic!("option choice")
    };
    options[0] = EffectOp::DrawCards {
        player: mtg_kernel::effect::PlayerRef::Controller,
        count: 1,
    };
    let mutated_before_step = pending.clone();
    assert!(engine::step(&mut pending, Action::ChooseEffectOption(0)).is_err());
    assert_eq!(pending, mutated_before_step);

    let (mut redirected, deem, a, b) = pending_deem_with_two_targets();
    engine::step(&mut redirected, Action::ChooseEffectOption(0)).unwrap();
    let continuation = redirected.engine.pending_effect.as_mut().unwrap();
    let rewrite = |frame: &mut EffectFrame| {
        let EffectFrame::OwnerLibraryPlacement { object, .. } = frame else {
            panic!("typed answered frame")
        };
        object.object = b;
    };
    rewrite(continuation.frames.last_mut().unwrap());
    let Some(EffectAnsweredChoiceGuard::OwnerLibrarySecondOrBottom { frame }) =
        continuation.answered_choice_guard.as_mut()
    else {
        panic!("answered guard")
    };
    rewrite(frame);
    let _ = engine::advance_until_decision(&mut redirected);
    assert_eq!(
        redirected.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, deem))
    );
    assert!(redirected.players[1].battlefield.contains(&a));
    assert!(redirected.players[1].battlefield.contains(&b));

    let (mut extra, deem, a, _) = pending_deem_with_two_targets();
    engine::step(&mut extra, Action::ChooseEffectOption(1)).unwrap();
    extra
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .frames
        .push(EffectFrame::Program {
            op: EffectOp::Sequence(vec![]),
            path: vec![99],
        });
    let _ = engine::advance_until_decision(&mut extra);
    assert_eq!(
        extra.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, deem))
    );
    assert!(extra.players[1].battlefield.contains(&a));

    let (mut answered_injection, deem, a, _) = pending_deem_with_two_targets();
    engine::step(&mut answered_injection, Action::ChooseEffectOption(0)).unwrap();
    let life_before = answered_injection.players[0].life;
    let continuation = answered_injection.engine.pending_effect.as_mut().unwrap();
    continuation.frames.insert(0, injected.clone());
    let EffectFrame::OwnerLibraryPlacement {
        expected_remaining_frames,
        ..
    } = continuation.frames.last_mut().unwrap()
    else {
        panic!("typed answered frame")
    };
    expected_remaining_frames.push(injected.clone());
    let Some(EffectAnsweredChoiceGuard::OwnerLibrarySecondOrBottom { frame }) =
        continuation.answered_choice_guard.as_mut()
    else {
        panic!("answered guard")
    };
    let EffectFrame::OwnerLibraryPlacement {
        expected_remaining_frames,
        ..
    } = frame.as_mut()
    else {
        panic!("typed answered guard frame")
    };
    expected_remaining_frames.push(injected);
    let _ = engine::advance_until_decision(&mut answered_injection);
    assert_eq!(
        answered_injection.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, deem))
    );
    assert_eq!(answered_injection.players[0].life, life_before);
    assert!(answered_injection.players[1].battlefield.contains(&a));
}

#[test]
fn spell_inline_effect_tamper_halts_before_executing_forged_program() {
    let (mut state, deem) = ready_deem(&[]);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    cast_and_target(&mut state, deem, target);
    state.stack.last_mut().unwrap().inline_effect = Some(EffectOp::GainLife {
        player: mtg_kernel::effect::PlayerRef::Controller,
        amount: 99,
    });
    let life_before = state.players[0].life;
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == deem
    ));
    assert_eq!(state.players[0].life, life_before);
    assert!(state.players[1].battlefield.contains(&target));
}

#[test]
fn fireblast_target_contract_is_captured_before_its_mountain_sacrifice_cost() {
    let p0 = [card_id("Island")];
    let mut state = GameState::new_from_libraries(&p0, &[], card_name, 0x4649_5245);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    let mountain_a = put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let mountain_b = put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    let fireblast = put_object(&mut state, PlayerId::P0, "Fireblast", Zone::Hand);
    engine::step(&mut state, Action::CastSpell(fireblast)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    let Decision::ChooseCostTargets {
        cost_kind,
        candidates,
        ..
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("expected Fireblast sacrifice choice")
    };
    assert_eq!(cost_kind, CostKind::SacrificeLands);
    assert!(candidates.contains(&mountain_a) && candidates.contains(&mountain_b));
    engine::step(&mut state, Action::ChooseCostTarget(mountain_a)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(mountain_a).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(mountain_b).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
    assert_eq!(state.stack[0].targets, vec![Target::Object(target)]);
    assert!(matches!(
        state.stack[0].v4.target_contracts.as_slice(),
        [StackTargetContractV4::Object { object, zone: Zone::Battlefield, zone_change_count: 0, .. }]
            if *object == target
    ));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(target, Zone::Graveyard),
    );
    pass_until_spell_finishes(&mut state, fireblast);
    assert_eq!(state.objects.get(target).damage, 0);
    assert_eq!(state.objects.get(fireblast).zone, Zone::Graveyard);
}

#[test]
fn leave_trigger_uses_last_known_controller_and_deem_is_not_a_dies_event() {
    let (mut state, _) = ready_deem(&["Mountain"]);
    let percussionist = put_object(
        &mut state,
        PlayerId::P0,
        "Clockwork Percussionist",
        Zone::Battlefield,
    );
    state.players[0]
        .battlefield
        .retain(|id| *id != percussionist);
    state.players[1].battlefield.push(percussionist);
    state.objects.get_mut(percussionist).controller = PlayerId::P1;
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(percussionist, Zone::Graveyard),
    );
    let triggers = trigger::collect_and_process(&mut state);
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].controller, PlayerId::P1);

    let (mut library_state, deem) = ready_deem(&[]);
    let target = put_object(
        &mut library_state,
        PlayerId::P1,
        "Clockwork Percussionist",
        Zone::Battlefield,
    );
    cast_and_target(&mut library_state, deem, target);
    advance_to_deem_choice(&mut library_state, deem);
    engine::step(&mut library_state, Action::ChooseEffectOption(1)).unwrap();
    let _ = engine::advance_until_decision(&mut library_state);
    assert!(!library_state.stack.iter().any(|item| item.source == target));
}

#[test]
fn malformed_stack_target_metadata_halts_instead_of_fizzling() {
    let (mut state, deem) = ready_deem(&[]);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    cast_and_target(&mut state, deem, target);
    state.stack.last_mut().unwrap().v4.target_contracts.clear();
    let stack_before = state.stack.clone();
    engine::step(&mut state, Action::Pass).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(
        state.engine.halted,
        Some((UnsupportedMechanic::InvalidEffectContinuation, deem))
    );
    assert_eq!(state.stack, stack_before);
    assert!(state.players[1].battlefield.contains(&target));
}

#[test]
fn contract_zone_and_future_generation_tampering_fail_before_projection_or_resolution() {
    let (mut bad_zone, zone_deem) = ready_deem(&[]);
    let zone_target = put_object(
        &mut bad_zone,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    cast_and_target(&mut bad_zone, zone_deem, zone_target);
    let StackTargetContractV4::Object { zone, .. } =
        &mut bad_zone.stack.last_mut().unwrap().v4.target_contracts[0]
    else {
        panic!("Deem has an object target contract")
    };
    *zone = Zone::Graveyard;
    assert_tampered_stack_target_halts(&mut bad_zone, zone_deem);

    let (mut future, future_deem) = ready_deem(&[]);
    let future_target = put_object(
        &mut future,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    cast_and_target(&mut future, future_deem, future_target);
    let live_generation = future.objects.get(future_target).zone_change_count;
    let StackTargetContractV4::Object {
        zone_change_count, ..
    } = &mut future.stack.last_mut().unwrap().v4.target_contracts[0]
    else {
        panic!("Deem has an object target contract")
    };
    *zone_change_count = live_generation + 1;
    assert_tampered_stack_target_halts(&mut future, future_deem);
}

#[test]
fn pending_cast_target_spec_tamper_halts_before_targeting_or_payment() {
    let (mut state, deem) = ready_deem(&[]);
    let target = put_object(
        &mut state,
        PlayerId::P1,
        "Cryptic Serpent",
        Zone::Battlefield,
    );
    engine::step(&mut state, Action::CastSpell(deem)).unwrap();
    state.engine.pending_cast.as_mut().unwrap().target_spec = TargetSpec::AnyTarget;
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == deem
    ));
    assert!(state.players[1].battlefield.contains(&target));
    assert_eq!(state.objects.get(deem).zone, Zone::Stack);
    assert_eq!(state.players[0].mana_pool[ManaColor::U.pool_index()], 1);
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 3);
}
