//! Actual future definition and accepted physical copy/attachment boundaries.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
use crate::card_def::card_id_by_name;
use crate::engine::{self, Action, Decision};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::state::{GameObject, ObjectStateV4, Step, Target};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("genuine future registration required");
    let object = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
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
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        _ => panic!("physical fixture supports hand/battlefield"),
    }
    object
}

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 12],
        &[forest; 12],
        |id| CARD_DEFS[id as usize].name.into(),
        11026,
        actor,
    );
    state.step = Step::Main1;
    state
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn finish_stack(state: &mut GameState) {
    for _ in 0..40 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected boundary {other:?}"),
        }
    }
    panic!("stack did not complete");
}

fn replay(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn genuine_copied_token_source_self_exile_never_returns_before_sba_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        // Physical entry covers the original printed ETB without inventing
        // a copy spell. Actual printed cast/payment is in the integration suite.
        let original = put(&mut state, actor, "Banishing Light", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(original, Zone::Battlefield),
        );
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(
            state.engine.pending_triggers.is_empty(),
            "no opponent nonland target"
        );
        // Genuine accepted allocator: CreateToken and the exact copiable
        // Banishing definition, with the allocator's physical token flag.
        let token =
            crate::standard_cards_v1::create_token_copy(&mut state, original, actor).unwrap();
        state.players[actor.index()]
            .battlefield
            .retain(|object| *object != token);
        state.players[actor.opponent().index()]
            .battlefield
            .push(token);
        state.objects.get_mut(token).controller = actor.opponent();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { player, legal_targets, .. }
            if player == actor && legal_targets.contains(&Target::Object(token)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(token))).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            finish_stack(branch);
            assert!(!branch
                .players
                .iter()
                .any(|player| player.battlefield.contains(&token)));
            assert!(
                !branch.exile.contains(&token),
                "real SBA ceases the exiled token"
            );
            assert!(!branch.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::ZoneChange { object, from: Zone::Exile, to: Zone::Battlefield, .. } if *object == token)));
            assert!(branch.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::BanishingLinkExpiredV1 { record, no_return_reason: Some(NoReturnV1::TokenCannotReturn), .. }
                    if record.exiled == token)));
            validate_state(branch).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn exiled_aura_without_host_stays_exiled_and_its_capture_cannot_be_erased_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let creature = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let aura = put(
            &mut state,
            actor.opponent(),
            "Twinblade Blessing",
            Zone::Battlefield,
        );
        state.attach_object_exact(aura, 0, creature, 0).unwrap();
        let source = put(&mut state, actor, "Banishing Light", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(aura)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(aura))).unwrap();
        finish_stack(&mut state);
        assert_eq!(state.objects.get(aura).zone, Zone::Exile);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Graveyard),
        );
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            assert_eq!(branch.objects.get(aura).zone, Zone::Exile);
            assert!(branch.exile.contains(&aura));
            assert!(branch.engine.linked_exile_records.is_empty());
            assert!(!branch.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::ZoneChange { object, from: Zone::Exile, to: Zone::Battlefield, .. } if *object == aura)));
            validate_state(branch).unwrap();
            let mut forged = replay(branch);
            forged.engine.event_history.retain(|event| {
                !matches!(event, CommittedEvent::BanishingAuraHostsCapturedV1 { .. })
            });
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(engine::step(&mut forged, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&forged).unwrap());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

pub(crate) fn linked_aura(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let mut state = ready(actor);
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
        put(&mut state, player, "Mountain", Zone::Battlefield);
    }
    let host = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let aura = put(
        &mut state,
        actor.opponent(),
        "Twinblade Blessing",
        Zone::Battlefield,
    );
    state.attach_object_exact(aura, 0, host, 0).unwrap();
    let source = put(&mut state, actor, "Banishing Light", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Battlefield),
    );
    assert!(
        matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(aura)))
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(aura))).unwrap();
    finish_stack(&mut state);
    assert_eq!(state.objects.get(aura).zone, Zone::Exile);
    (state, source, aura, host)
}

fn assert_owner_choice(state: &mut GameState, owner: PlayerId, aura: ObjectId, host: ObjectId) {
    assert!(matches!(next(state), Decision::ChooseEffectTargets {
        player, source, selected_count: 0, min_targets: 1, max_targets: 1,
        legal_targets, can_finish: false,
    } if player==owner && source==aura && legal_targets.contains(&Target::Object(host))));
    validate_state(state).unwrap();
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn actual_source_departure_returns_aura_by_untargeted_owner_choice_with_shroud_and_hexproof() {
    use crate::card_def::Keywords;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for keyword in [Keywords::SHROUD, Keywords::HEXPROOF] {
            let (mut state, source, aura, host) = linked_aura(actor);
            let host_generation = state.objects.get(host).zone_change_count;
            crate::effect::install_temporary_boost(
                &mut state,
                crate::effect::EffectObjectBinding {
                    object: host,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: host_generation,
                },
                0,
                0,
                keyword,
            );
            let targeted = state
                .engine
                .event_history
                .iter()
                .filter(|event| matches!(event, CommittedEvent::Targeted { .. }))
                .count();
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            assert_owner_choice(&mut state, actor.opponent(), aura, host);
            assert_eq!(state.objects.get(source).zone, Zone::Hand);
            assert_eq!(state.objects.get(aura).zone, Zone::Exile);
            let mut restored = replay(&state);
            for branch in [&mut state, &mut restored] {
                let before = serde_json::to_vec(branch).unwrap();
                assert!(engine::step(branch, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(branch).unwrap(), before);
                engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
                assert_eq!(branch.objects.get(aura).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(aura).controller, actor.opponent());
                assert_eq!(
                    branch.objects.get(aura).v4.attached_to.unwrap().object,
                    host
                );
                assert_eq!(
                    branch
                        .engine
                        .event_history
                        .iter()
                        .filter(|event| matches!(event, CommittedEvent::Targeted { .. }))
                        .count(),
                    targeted
                );
                validate_state(branch).unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn real_broken_wings_resolution_is_owned_until_attachment_and_departure_then_restores_exactly() {
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, aura, host) = linked_aura(actor);
        let spell = put(&mut state, actor, "Broken Wings", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 3;
        for _ in 0..4 {
            if matches!(next(&mut state),Decision::CastSpellOrPass {player,..} if player==actor) {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(
            matches!(next(&mut state),Decision::ChooseTargets {legal_targets,..}
            if legal_targets.contains(&Target::Object(source)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        for _ in 0..8 {
            match next(&mut state) {
                Decision::ChooseEffectTargets {
                    source: visible, ..
                } if visible == aura => break,
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("{other:?}"),
            }
        }
        assert_owner_choice(&mut state, actor.opponent(), aura, host);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(spell).zone, Zone::Stack);
        assert!(
            state.stack.iter().any(|item| item.source == spell),
            "genuine resolving spell remains visible"
        );
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event,
            CommittedEvent::ZoneChange {object,from:Zone::Stack,..} if *object==spell)));
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
            assert_eq!(branch.objects.get(aura).zone, Zone::Battlefield);
            assert_eq!(
                branch.objects.get(aura).v4.attached_to.unwrap().object,
                host
            );
            assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(branch.engine.event_history.iter().filter(|event|matches!(event,
                CommittedEvent::ZoneChange {object,from:Zone::Stack,to:Zone::Graveyard,..} if *object==spell)).count(),1);
            validate_state(branch).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn restored_owner_checkpoint_erasure_coherent_menu_and_operation_rewrites_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, aura, host) = linked_aura(actor);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        assert_owner_choice(&mut state, actor.opponent(), aura, host);
        for mode in 0..7 {
            let mut bad = replay(&state);
            match mode {
                0 => bad.engine.linked_aura_operation_v1 = Default::default(),
                1 => {
                    bad.engine.linked_aura_operation_v1 = Default::default();
                    bad.engine.event_history.retain(|event| {
                        !matches!(event, CommittedEvent::BanishingAuraHostsCapturedV1 { .. })
                    });
                }
                2 => {
                    bad.engine
                        .linked_aura_operation_v1
                        .pending
                        .as_mut()
                        .unwrap()
                        .choice
                        .owner = actor
                }
                3 => {
                    bad.engine
                        .linked_aura_operation_v1
                        .pending
                        .as_mut()
                        .unwrap()
                        .operation =
                        crate::linked_aura_operation_v1::OperationV1::Step(Action::Pass)
                }
                4 => bad.objects.get_mut(host).zone_change_count += 1,
                5 => {
                    bad.engine
                        .linked_aura_operation_v1
                        .pending
                        .as_mut()
                        .unwrap()
                        .operation = crate::linked_aura_operation_v1::OperationV1::Commit(
                        ProposedEvent::zone_change(source, Zone::Hand),
                    )
                }
                _ => {
                    bad.engine
                        .linked_aura_operation_v1
                        .pending
                        .as_mut()
                        .unwrap()
                        .original
                        .players[actor.index()]
                    .mana_pool = [3; 6];
                    bad.players[actor.index()].mana_pool = [3; 6];
                }
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(
                engine::step(&mut bad, Action::ChooseEffectTarget(Target::Object(host))).is_err()
            );
            assert_eq!(before, serde_json::to_vec(&bad).unwrap());
        }
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn accepted_physical_sequence_stops_before_draw_and_shuffle_and_replays_rng_once() {
    use crate::effect::{EffectOp, ExecCtx, ObjectRef, PlayerRef};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, aura, host) = linked_aura(actor);
        let hand_before = state.players[actor.index()].hand.len();
        let library_before = state.players[actor.index()].library.clone();
        // Accepted physical opcode boundary, not a fabricated printed spell.
        let op = EffectOp::Sequence(vec![
            EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Hand,
            },
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            EffectOp::LookTopTakeCreaturesManaValueAtMostThenShuffle {
                count: 0,
                max_taken: 0,
                max_mana_value: 0,
            },
        ]);
        crate::effect::execute(&op, &ExecCtx::no_targets(source, actor), &mut state);
        assert_owner_choice(&mut state, actor.opponent(), aura, host);
        assert_eq!(
            state.players[actor.index()].hand.len(),
            hand_before + 1,
            "only the departed source moved"
        );
        assert_eq!(
            state.players[actor.index()].library,
            library_before,
            "tail has not drawn/shuffled"
        );
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
            assert_eq!(branch.players[actor.index()].hand.len(), hand_before + 2);
            assert_eq!(
                branch.players[actor.index()].library.len(),
                library_before.len() - 1
            );
            assert_eq!(
                branch.objects.get(aura).v4.attached_to.unwrap().object,
                host
            );
            validate_state(branch).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn simultaneous_departures_complete_before_returns_and_keep_prior_owner_answers_on_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, first_source, first_aura, host) = linked_aura(actor);
        let second_aura = put(
            &mut state,
            actor.opponent(),
            "Twinblade Blessing",
            Zone::Battlefield,
        );
        state
            .attach_object_exact(
                second_aura,
                0,
                host,
                state.objects.get(host).zone_change_count,
            )
            .unwrap();
        let second_source = put(&mut state, actor, "Banishing Light", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(second_source, Zone::Battlefield),
        );
        assert!(
            matches!(next(&mut state),Decision::ChooseTargets {legal_targets,..}
            if legal_targets.contains(&Target::Object(second_aura)))
        );
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Object(second_aura)),
        )
        .unwrap();
        finish_stack(&mut state);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(first_source, Zone::Hand),
                ProposedEvent::zone_change(second_source, Zone::Hand),
            ],
        );
        assert_owner_choice(&mut state, actor.opponent(), first_aura, host);
        assert_eq!(state.objects.get(first_source).zone, Zone::Hand);
        assert_eq!(
            state.objects.get(second_source).zone,
            Zone::Hand,
            "entire simultaneous departure preceded return choice"
        );
        engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
        assert_owner_choice(&mut state, actor.opponent(), second_aura, host);
        assert_eq!(state.objects.get(first_aura).zone, Zone::Exile);
        assert_eq!(
            state
                .engine
                .event_history
                .iter()
                .filter(|event| matches!(
                    event,
                    CommittedEvent::BanishingAuraHostsCapturedV1 { .. }
                ))
                .count(),
            2
        );
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event, CommittedEvent::BanishingAuraHostChosenV1 { .. })));
        assert_eq!(state.objects.get(second_aura).zone, Zone::Exile);
        assert_eq!(
            state
                .engine
                .linked_aura_operation_v1
                .pending
                .as_ref()
                .unwrap()
                .answers
                .len(),
            1
        );
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
            for aura in [first_aura, second_aura] {
                assert_eq!(branch.objects.get(aura).zone, Zone::Battlefield);
                assert_eq!(
                    branch.objects.get(aura).v4.attached_to.unwrap().object,
                    host
                );
                assert_eq!(branch.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::ZoneChange {object,from:Zone::Exile,to:Zone::Battlefield,..} if *object==aura)).count(),1);
            }
            validate_state(branch).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn real_registered_activation_cost_pauses_before_publication_at_physical_copy_boundary() {
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, aura, host) = linked_aura(actor);
        let heartfire = card_id_by_name("Heartfire Immolator").expect("actual admitted operand");
        let generation = state.objects.get(source).zone_change_count;
        let original = state.objects.get(source).card_def;
        // Physical copiable-characteristics substrate only. This does not
        // claim an admitted printed producer which makes this copy in FDN.
        state
            .standard_v1
            .get_or_insert_with(Default::default)
            .copied_card_defs
            .extend([
                (source, generation, original),
                (source, generation, heartfire),
            ]);
        let live = state.objects.get_mut(source);
        live.card_def = heartfire;
        live.name = CARD_DEFS[heartfire as usize].object_name.into();
        live.v4.effective_color_mask = ObjectStateV4::from_card_def(heartfire).effective_color_mask;
        live.v4.effective_subtype_ids =
            ObjectStateV4::from_card_def(heartfire).effective_subtype_ids;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        for _ in 0..4 {
            if matches!(next(&mut state),Decision::CastSpellOrPass {player,..} if player==actor) {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(
            matches!(next(&mut state),Decision::ChooseTargets {legal_targets,..}
            if legal_targets.contains(&Target::Object(host)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
        assert_owner_choice(&mut state, actor.opponent(), aura, host);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
            0
        );
        assert!(
            !state.stack.iter().any(|item| item.source == source),
            "cost owner has not published activation"
        );
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
            assert_eq!(branch.objects.get(aura).zone, Zone::Battlefield);
            assert_eq!(
                branch.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
                0
            );
            let activated = branch
                .stack
                .iter()
                .find(|item| item.source == source)
                .expect("original paid activation publishes after return");
            assert_eq!(activated.targets, [Target::Object(host)]);
            assert_eq!(activated.controller, actor);
            validate_state(branch).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn simultaneous_creature_and_aura_returns_use_only_pre_entry_hosts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for alternate_host in [false, true] {
            let (mut state, aura_source, aura, returning_creature) = linked_aura(actor);
            let creature_source = put(&mut state, actor, "Banishing Light", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature_source, Zone::Battlefield),
            );
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets {legal_targets,..}
                if legal_targets.contains(&Target::Object(returning_creature)))
            );
            engine::step(
                &mut state,
                Action::ChooseTarget(Target::Object(returning_creature)),
            )
            .unwrap();
            finish_stack(&mut state);
            assert_eq!(state.objects.get(returning_creature).zone, Zone::Exile);
            let host =
                alternate_host.then(|| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield));
            // Creature source first deliberately reproduces the prior leak.
            event::propose_and_commit_batch(
                &mut state,
                vec![
                    ProposedEvent::zone_change(creature_source, Zone::Hand),
                    ProposedEvent::zone_change(aura_source, Zone::Hand),
                ],
            );
            if let Some(host) = host {
                assert_owner_choice(&mut state, actor.opponent(), aura, host);
                assert_eq!(state.objects.get(returning_creature).zone, Zone::Exile);
                let Decision::ChooseEffectTargets { legal_targets, .. } = next(&mut state) else {
                    panic!("owner choice absent")
                };
                assert!(
                    !legal_targets.contains(&Target::Object(returning_creature)),
                    "a creature entering simultaneously cannot be an Aura host"
                );
                let mut restored = replay(&state);
                for branch in [&mut state, &mut restored] {
                    engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
                    assert_eq!(
                        branch.objects.get(returning_creature).zone,
                        Zone::Battlefield
                    );
                    assert_eq!(branch.objects.get(aura).zone, Zone::Battlefield);
                    assert_eq!(
                        branch.objects.get(aura).v4.attached_to.unwrap().object,
                        host
                    );
                    validate_state(branch).unwrap();
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
            } else {
                assert!(!crate::linked_aura_operation_v1::has_pending(&state));
                assert_eq!(
                    state.objects.get(returning_creature).zone,
                    Zone::Battlefield
                );
                assert_eq!(
                    state.objects.get(aura).zone,
                    Zone::Exile,
                    "the returned creature does not rescue an Aura with no pre-entry host"
                );
                assert!(state
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event,
                    CommittedEvent::BanishingAuraHostsCapturedV1 {choice}
                        if choice.aura.object == aura && choice.original_candidates.is_empty())));
                validate_state(&replay(&state)).unwrap();
            }
        }
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn simultaneous_opposite_owner_choices_follow_apnap_before_any_entry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            let (mut state, opponent_aura_source, opponent_aura, host) = linked_aura(actor);
            let active_aura = put(&mut state, actor, "Twinblade Blessing", Zone::Battlefield);
            state
                .attach_object_exact(
                    active_aura,
                    0,
                    host,
                    state.objects.get(host).zone_change_count,
                )
                .unwrap();
            let active_aura_source =
                put(&mut state, actor.opponent(), "Banishing Light", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(active_aura_source, Zone::Battlefield),
            );
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets {player,legal_targets,..}
                if player == actor.opponent() && legal_targets.contains(&Target::Object(active_aura)))
            );
            engine::step(
                &mut state,
                Action::ChooseTarget(Target::Object(active_aura)),
            )
            .unwrap();
            finish_stack(&mut state);
            assert_eq!(state.active_player, actor);
            let mut events = vec![
                ProposedEvent::zone_change(opponent_aura_source, Zone::Hand),
                ProposedEvent::zone_change(active_aura_source, Zone::Hand),
            ];
            if reverse {
                events.reverse();
            }
            event::propose_and_commit_batch(&mut state, events);
            assert_owner_choice(&mut state, actor, active_aura, host);
            assert_eq!(state.objects.get(active_aura).zone, Zone::Exile);
            assert_eq!(state.objects.get(opponent_aura).zone, Zone::Exile);
            engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
            assert_owner_choice(&mut state, actor.opponent(), opponent_aura, host);
            assert_eq!(state.objects.get(active_aura).zone, Zone::Exile);
            assert_eq!(state.objects.get(opponent_aura).zone, Zone::Exile);
            let mut restored = replay(&state);
            for branch in [&mut state, &mut restored] {
                engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
                for aura in [active_aura, opponent_aura] {
                    assert_eq!(branch.objects.get(aura).zone, Zone::Battlefield);
                    assert_eq!(
                        branch.objects.get(aura).v4.attached_to.unwrap().object,
                        host
                    );
                }
                validate_state(branch).unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "requires genuine future Banishing Light registration and V26 qualification"]
fn actual_aura_choice_public_human_owner_restore_retry_and_forgery_both_seats() {
    use crate::human_bo3_v1::*;
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, aura, host) = linked_aura(actor);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        let owner = actor.opponent();
        let backend = FastActorSessionV1::from_banishing_v26_fixture_state(state);
        let mut session = HumanBanishingSessionV26::from_session(backend).unwrap();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("actual owner choice")
        };
        let mut projector = HumanBanishingProjectorV26::new(owner.into());
        let prompt = projector.project_current(&session, expected).unwrap();
        let choice = prompt.state.extensions.attachment_choice.as_ref().unwrap();
        assert_eq!(choice.owner, owner.into());
        assert!(choice.untargeted && choice.mandatory);
        let public = session.public_observation(expected).unwrap();
        assert_eq!(
            public.attachment_choice.as_ref().unwrap().aura.arena_id,
            aura.0
        );
        assert!(public
            .attachment_choice
            .as_ref()
            .unwrap()
            .legal_hosts
            .iter()
            .any(|row| row.arena_id == host.0));
        let value = serde_json::to_value(&prompt).unwrap();
        let text = serde_json::to_string(&value).unwrap();
        for forbidden in [
            "arena_id",
            "card_db_id",
            "event_history",
            "original_sha256",
            "capture_history_index",
            "engine_indexes",
        ] {
            assert!(!text.contains(forbidden), "private field {forbidden}");
        }
        let snapshot = session.snapshot_v26();
        let input = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: prompt
                .actions
                .iter()
                .find(|row| row.label.starts_with("Attach returning Twinblade Blessing"))
                .unwrap()
                .action_index,
        };
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        let after = session
            .public_observation(match session.current_response() {
                FastActorResponseV1::Decision(d) => d,
                _ => panic!("post-return priority"),
            })
            .unwrap();
        assert!(after.links.iter().all(|row| row.expired));
        session.restore_v26(&snapshot).unwrap();
        assert_eq!(
            projector.submit(&mut session, input),
            Err(HumanDecisionErrorV1::StaleDecision)
        );
        let FastActorResponseV1::Decision(refreshed) = session.current_response() else {
            panic!("restored owner")
        };
        let fresh = projector.project_current(&session, refreshed).unwrap();
        assert!(fresh.prompt_seq > prompt.prompt_seq);
        assert_eq!(fresh.state, prompt.state);
        assert_eq!(fresh.actions, prompt.actions);
        let mut forged = public.clone();
        forged.attachment_choice.as_mut().unwrap().owner = actor.into();
        assert!(forged.validate_artifact().is_err());
    }
}

#[test]
#[ignore = "requires genuine future Banishing Light registration and V26 qualification"]
fn later_apnap_owner_sees_confirmed_answer_before_entry_and_restores_it_both_seats() {
    use crate::human_bo3_v1::*;
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, opponent_source, opponent_aura, host) = linked_aura(actor);
        let active_aura = put(&mut state, actor, "Twinblade Blessing", Zone::Battlefield);
        state
            .attach_object_exact(
                active_aura,
                0,
                host,
                state.objects.get(host).zone_change_count,
            )
            .unwrap();
        let active_source = put(&mut state, actor.opponent(), "Banishing Light", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(active_source, Zone::Battlefield),
        );
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Object(active_aura)),
        )
        .unwrap();
        finish_stack(&mut state);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(opponent_source, Zone::Hand),
                ProposedEvent::zone_change(active_source, Zone::Hand),
            ],
        );
        let backend = FastActorSessionV1::from_banishing_v26_fixture_state(state);
        let mut session = HumanBanishingSessionV26::from_session(backend).unwrap();
        let mut active = HumanBanishingProjectorV26::new(actor.into());
        let FastActorResponseV1::Decision(first) = session.current_response() else {
            panic!("first owner")
        };
        let first_prompt = active.project_current(&session, first).unwrap();
        active
            .submit(
                &mut session,
                HumanActionRequestV1 {
                    prompt_seq: first_prompt.prompt_seq,
                    action_index: first_prompt.actions[0].action_index,
                },
            )
            .unwrap();
        let FastActorResponseV1::Decision(second) = session.current_response() else {
            panic!("second owner")
        };
        assert_eq!(second.acting_player, actor.opponent().into());
        let mut opponent = HumanBanishingProjectorV26::new(actor.opponent().into());
        let second_prompt = opponent.project_current(&session, second).unwrap();
        assert_eq!(
            second_prompt.state.extensions.confirmed_attachments.len(),
            1
        );
        let prior = &second_prompt.state.extensions.confirmed_attachments[0];
        assert_eq!(prior.owner, actor.into());
        assert!(prior.current_chosen_host.is_some());
        let public = session.public_observation(second).unwrap();
        assert_eq!(public.confirmed_attachments[0].aura.arena_id, active_aura.0);
        assert_eq!(
            public.attachment_choice.as_ref().unwrap().aura.arena_id,
            opponent_aura.0
        );
        assert_eq!(public.confirmed_attachments[0].aura.zone, Zone::Exile);
        let snapshot = session.snapshot_v26();
        session.restore_v26(&snapshot).unwrap();
        let FastActorResponseV1::Decision(restored) = session.current_response() else {
            panic!("restored second owner")
        };
        let restored_prompt = opponent.project_current(&session, restored).unwrap();
        assert_eq!(restored_prompt.state, second_prompt.state);
        assert_eq!(restored_prompt.actions, second_prompt.actions);
        opponent
            .submit(
                &mut session,
                HumanActionRequestV1 {
                    prompt_seq: restored_prompt.prompt_seq,
                    action_index: restored_prompt.actions[0].action_index,
                },
            )
            .unwrap();
        let FastActorResponseV1::Decision(completed) = session.current_response() else {
            panic!("return priority")
        };
        assert!(session
            .public_observation(completed)
            .unwrap()
            .links
            .iter()
            .all(|row| row.expired));
    }
}

#[test]
#[ignore = "requires genuine future Banishing Light registration and V26 qualification"]
fn actual_printed_cast_and_etb_meaning_public_binding_both_seats() {
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let source = put(&mut state, actor, "Banishing Light", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        for _ in 0..12 {
            match next(&mut state) {
                Decision::ChooseTargets {
                    player,
                    legal_targets,
                    ..
                } => {
                    assert_eq!(player, actor);
                    assert!(legal_targets.contains(&Target::Object(target)));
                    break;
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual ETB placement {other:?}"),
            }
        }
        let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
        let observation =
            crate::banishing_public_v1::observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(observation.roots.len(), 3);
        assert_eq!(observation.triggers.len(), 1);
        assert_eq!(observation.triggers[0].captured_controller, actor.into());
        assert_eq!(observation.triggers[0].source_at_entry.arena_id, source.0);
        assert!(crate::policy_observation_v9::observe_policy_v9(
            &state, &surface, actor, 0, 0, 0, 1
        )
        .is_err());
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        finish_stack(&mut state);
        let observation =
            crate::banishing_public_v1::observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(observation.links.len(), 1);
        assert_eq!(
            observation.links[0].recipient_owner,
            actor.opponent().into()
        );
        assert!(!observation.links[0].expired);
        assert_eq!(state.objects.get(target).zone, Zone::Exile);
    }
}
