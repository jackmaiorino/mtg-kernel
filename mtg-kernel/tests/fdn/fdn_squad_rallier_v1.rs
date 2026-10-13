//! Actual registered Squad Rallier fixtures on the current Limited catalog.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, CostComponent, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{
    EffectFrame, EffectOp, LibraryLookCount, LibraryPickFilterV1, PendingEffectChoice,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(actor: PlayerId, names: &[&str]) -> GameState {
    ready_with_starting_player(actor, names, actor)
}
fn ready_with_starting_player(actor: PlayerId, names: &[&str], starting: PlayerId) -> GameState {
    let cards: Vec<_> = names
        .iter()
        .map(|name| card_id_by_name(name).unwrap())
        .collect();
    let forest = card_id_by_name("Forest").unwrap();
    let opponent_cards = [forest; 40];
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        if actor == PlayerId::P0 {
            &cards
        } else {
            &opponent_cards
        },
        if actor == PlayerId::P1 {
            &cards
        } else {
            &opponent_cards
        },
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_331,
        starting,
    );
    state.step = Step::Main1;
    state
}
fn put(state: &mut GameState, owner: PlayerId, name: &str) -> ObjectId {
    let card_def = card_id_by_name(name).expect("future actual registered definition");
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Hand,
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
    state.players[owner.index()].hand.push(id);
    id
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn settle(state: &mut GameState) {
    for _ in 0..60 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("did not settle");
}
fn until_pick(state: &mut GameState) -> Decision {
    for _ in 0..60 {
        let decision = next(state);
        match decision {
            Decision::ChooseEffectTargets { .. } => return decision,
            Decision::CastSpellOrPass { .. } if !state.stack.is_empty() => {
                engine::step(state, Action::Pass).unwrap()
            }
            other => panic!("expected paid look, got {other:?}"),
        }
    }
    panic!("did not reach pick");
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn readonly(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}
fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Squad Rallier");
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 4;
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    source
}
fn activate(state: &mut GameState, actor: PlayerId, source: ObjectId) {
    state.priority_player = actor;
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 3;
    next(state);
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_activation.is_none());
    let item = state
        .stack
        .last()
        .expect("actual finalized paid activation");
    assert_eq!(
        item.kind,
        mtg_kernel::state::StackItemKind::ActivatedAbility
    );
    assert_eq!((item.source, item.controller), (source, actor));
    assert!(
        item.v4.squad_rallier_started_v1.is_empty(),
        "genuine unstarted activation remains unmarked"
    );
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()],
        0
    );
}
fn transfer(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    // Registered control primitive stand-in, no fabricated printed producer.
    let previous = state.objects.get(object).controller;
    state.players[previous.index()]
        .battlefield
        .retain(|id| *id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}
fn sort_ids(mut ids: Vec<ObjectId>) -> Vec<ObjectId> {
    ids.sort();
    ids
}

#[test]
fn real_cast_optional_power_pick_reveal_random_bottom_and_restored_replay_both_seats() {
    let def =
        &CARD_DEFS[card_id_by_name("Squad Rallier").expect("future FDN registration") as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!((def.power, def.toughness), (Some(3), Some(4)));
    assert_eq!(def.cost.generic, 3);
    let ability = &def.activated_abilities[0];
    assert_eq!(ability.target_spec, TargetSpec::None);
    assert!(matches!(ability.cost, [CostComponent::Mana(cost)] if cost.generic == 2));
    assert!(matches!((ability.effect)(), EffectOp::LookTopSelectV1 {
        count: LibraryLookCount::Fixed(4), rule, pick_x: false, ..
    } if rule.pick == 1 && !rule.choose_rest_order && rule.selection.is_some_and(|s|
        s.filter == LibraryPickFilterV1::CreaturePowerAtMost(2) && s.optional && s.reveal_selected && s.destination == Zone::Hand)));
    for actor in [PlayerId::P0, PlayerId::P1] {
        for take in [false, true] {
            let mut state = ready(
                actor,
                &[
                    "Burning-Tree Emissary",
                    "Llanowar Elves",
                    "Tolarian Terror",
                    "Forest",
                    "Forest",
                    "Counterspell",
                ],
            );
            let original = state.players[actor.index()].library.clone();
            let source = cast(&mut state, actor);
            // No tap or sickness restriction: activate the real newly cast creature.
            activate(&mut state, actor, source);
            let decision = until_pick(&mut state);
            assert_eq!(state.known_library_cards(actor, actor).len(), 4);
            assert!(state
                .known_library_cards(actor.opponent(), actor)
                .is_empty());
            assert!(matches!(decision, Decision::ChooseEffectTargets {
                player, source: current, selected_count: 0, min_targets: 0, max_targets: 1,
                ref legal_targets, can_finish: true,
            } if player == actor && current == source && *legal_targets == vec![Target::Object(original[0]), Target::Object(original[1])]));
            let random_before =
                serde_json::to_vec(state.legacy_rng().expect("legacy fixture")).unwrap();
            let mut restored = restore(&state);
            for current in [&mut state, &mut restored] {
                readonly(current, Action::ChooseTarget(Target::Object(original[0])));
                readonly(
                    current,
                    Action::ChooseEffectTarget(Target::Object(original[2])),
                );
                readonly(
                    current,
                    Action::ChooseEffectTarget(Target::Object(original[4])),
                );
                if take {
                    engine::step(
                        current,
                        Action::ChooseEffectTarget(Target::Object(original[0])),
                    )
                    .unwrap();
                } else {
                    engine::step(current, Action::FinishEffectSelection).unwrap();
                }
                settle(current);
                assert_eq!(current.players[actor.index()].library[..2], original[4..]);
                let expected_rest = if take {
                    original[1..4].to_vec()
                } else {
                    original[..4].to_vec()
                };
                assert_eq!(
                    sort_ids(current.players[actor.index()].library[2..].to_vec()),
                    sort_ids(expected_rest)
                );
                assert_ne!(
                    serde_json::to_vec(current.legacy_rng().expect("legacy fixture")).unwrap(),
                    random_before
                );
                assert_eq!(
                    current.players[actor.index()].hand.contains(&original[0]),
                    take
                );
                assert_eq!(current.objects.get(original[0]).owner, actor);
                // Own hand visibility is represented by the hand itself.
                // Revealing a selected card adds knowledge for the opponent.
                assert!(current.known_hand_cards(actor, actor).is_empty());
                assert_eq!(
                    current
                        .known_hand_cards(actor.opponent(), actor)
                        .iter()
                        .any(|entry| entry.object == original[0]),
                    take
                );
                assert!(current.engine.event_log.iter().all(|event| !matches!(
                    event,
                    CommittedEvent::SquadRallierResolutionBoundaryV1 { .. }
                )));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn real_ability_uses_captured_controller_library_after_source_response_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for response in 0..3 {
            let mut state = ready(
                actor,
                &["Llanowar Elves", "Forest", "Forest", "Forest", "Forest"],
            );
            let original = state.players[actor.index()].library.clone();
            let other_library = state.players[actor.opponent().index()].library.clone();
            let source = cast(&mut state, actor);
            activate(&mut state, actor, source);
            next(&mut state);
            match response {
                0 => transfer(&mut state, source, actor.opponent()),
                1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                }
                _ => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
            }
            state = restore(&state);
            assert!(
                matches!(until_pick(&mut state), Decision::ChooseEffectTargets { player, .. } if player == actor)
            );
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(original[0])),
            )
            .unwrap();
            settle(&mut state);
            assert_eq!(state.objects.get(original[0]).zone, Zone::Hand);
            assert!(state.players[actor.index()].hand.contains(&original[0]));
            assert_eq!(
                state.players[actor.opponent().index()].library,
                other_library
            );
        }
    }
}

#[test]
fn stolen_physical_source_uses_activator_library_and_short_payment_wrong_controller_refuse() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let owner = actor.opponent();
        let mut state = ready_with_starting_player(
            actor,
            &[
                "Forest",
                "Llanowar Elves",
                "Forest",
                "Forest",
                "Forest",
                "Forest",
            ],
            owner,
        );
        let source = cast(&mut state, owner);
        super::fixture_turn_v1::reach(&mut state, actor, Step::Main1);
        assert_eq!(
            state
                .objects
                .get(*state.players[actor.index()].hand.last().unwrap())
                .card_def,
            card_id_by_name("Forest").unwrap()
        );
        let original = state.players[actor.index()].library.clone();
        assert_eq!(
            state.objects.get(original[0]).card_def,
            card_id_by_name("Llanowar Elves").unwrap()
        );
        transfer(&mut state, source, actor);
        assert_eq!(state.objects.get(source).owner, owner);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
        next(&mut state);
        readonly(&mut state, Action::ActivateAbility(source, 0));
        state.priority_player = owner;
        state.players[owner.index()].mana_pool[ManaColor::W.pool_index()] = 3;
        next(&mut state);
        readonly(&mut state, Action::ActivateAbility(source, 0));
        activate(&mut state, actor, source);
        let mut restored = restore(&state);
        for current in [&mut state, &mut restored] {
            assert!(
                matches!(until_pick(current), Decision::ChooseEffectTargets { player, .. } if player == actor)
            );
            engine::step(
                current,
                Action::ChooseEffectTarget(Target::Object(original[0])),
            )
            .unwrap();
            settle(current);
            assert_eq!(current.objects.get(source).owner, owner);
            assert_eq!(current.objects.get(original[0]).owner, actor);
            assert!(current.players[actor.index()].hand.contains(&original[0]));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn empty_or_no_eligible_prefix_finishes_real_paid_random_bottom_without_targets() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for names in [
            &[][..],
            &["Forest", "Counterspell", "Tolarian Terror", "Forest"][..],
        ] {
            let mut state = ready(actor, names);
            let original = state.players[actor.index()].library.clone();
            let source = cast(&mut state, actor);
            activate(&mut state, actor, source);
            let mut restored = restore(&state);
            for current in [&mut state, &mut restored] {
                if !names.is_empty() {
                    assert!(
                        matches!(until_pick(current), Decision::ChooseEffectTargets {
                        player, source: current_source, selected_count: 0,
                        min_targets: 0, max_targets: 0, ref legal_targets, can_finish: true,
                    } if player == actor && current_source == source && legal_targets.is_empty())
                    );
                    readonly(current, Action::ChooseEffectTarget(Target::Object(source)));
                    engine::step(current, Action::FinishEffectSelection).unwrap();
                }
                settle(current);
                assert_eq!(
                    sort_ids(current.players[actor.index()].library.clone()),
                    sort_ids(original.clone())
                );
                assert!(current.players[actor.index()].hand.is_empty());
                assert!(current.engine.pending_effect.is_none());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn restored_private_stage_erasure_restart_controller_filter_and_prefix_forgery_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(
            actor,
            &[
                "Llanowar Elves",
                "Forest",
                "Forest",
                "Forest",
                "Counterspell",
            ],
        );
        let source = cast(&mut state, actor);
        activate(&mut state, actor, source);
        until_pick(&mut state);
        let root = (CARD_DEFS[state.objects.get(source).card_def as usize].activated_abilities[0]
            .effect)();
        let top = state.players[actor.index()].library[0];
        for failure in 0..12 {
            let mut forged = restore(&state);
            match failure {
                0 => {
                    let pending = forged.engine.pending_effect.as_mut().unwrap();
                    pending.choice = None;
                    pending.frames.clear();
                    pending.answered_choice_guard = None;
                }
                1 => {
                    let pending = forged.engine.pending_effect.as_mut().unwrap();
                    pending.choice = None;
                    pending.frames = vec![EffectFrame::Program {
                        op: root.clone(),
                        path: Vec::new(),
                    }];
                    pending.answered_choice_guard = None;
                }
                2 => {
                    forged.engine.pending_effect = None;
                }
                3 => {
                    let pending = forged.engine.pending_effect.as_mut().unwrap();
                    pending.ctx.controller = actor.opponent();
                    pending.resolving_item.controller = actor.opponent();
                }
                4 => {
                    let pending = forged.engine.pending_effect.as_mut().unwrap();
                    if let Some(PendingEffectChoice::SelectTargets { legal, .. }) =
                        &mut pending.choice
                    {
                        legal.clear();
                    }
                }
                5 => {
                    forged.players[actor.index()].library.swap(0, 1);
                }
                6 => {
                    forged.objects.get_mut(top).zone_change_count += 1;
                }
                7 => {
                    forged.engine.pending_effect = None;
                    forged.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            CommittedEvent::SquadRallierResolutionBoundaryV1 { .. }
                        )
                    });
                }
                8 => {
                    forged
                        .stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .squad_rallier_started_v1
                        .0 = None;
                }
                9 => {
                    forged
                        .stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .squad_rallier_started_v1
                        .0
                        .as_mut()
                        .unwrap()
                        .first_receipt_history_index += 1;
                }
                10 => {
                    forged
                        .stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .squad_rallier_started_v1
                        .0
                        .as_mut()
                        .unwrap()
                        .source
                        .controller = actor.opponent();
                }
                _ => {
                    forged.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            CommittedEvent::SquadRallierResolutionBoundaryV1 { .. }
                        )
                    });
                }
            }
            forged = restore(&restore(&forged));
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());
            readonly(&mut forged, Action::FinishEffectSelection);
            readonly(&mut forged, Action::Pass);
            // Action rejection is read-only. Advancing records the terminal
            // diagnostic; every other serialized field must remain identical.
            let mut expected_halt = restore(&forged);
            expected_halt.engine.halted = Some((
                engine::UnsupportedMechanic::InvalidEffectContinuation,
                source,
            ));
            assert!(matches!(
                engine::advance_until_decision(&mut forged),
                Decision::Halted {
                    mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation,
                    source: halted_source,
                } if halted_source == source
            ));
            assert_eq!(
                serde_json::to_vec(&forged).unwrap(),
                serde_json::to_vec(&expected_halt).unwrap(),
                "forgery case {failure} changed more than the halt diagnostic"
            );
            readonly(&mut forged, Action::FinishEffectSelection);
            readonly(&mut forged, Action::Pass);
        }
    }
}

#[test]
fn real_registered_begin_answer_resume_retains_owned_mark_after_repeated_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for take in [false, true] {
            let mut state = ready(
                actor,
                &[
                    "Llanowar Elves",
                    "Forest",
                    "Forest",
                    "Forest",
                    "Counterspell",
                ],
            );
            let source = cast(&mut state, actor);
            activate(&mut state, actor, source);
            state = restore(&restore(&state));
            until_pick(&mut state);
            let pending = state.engine.pending_effect.as_ref().unwrap();
            let started = pending.resolving_item.v4.squad_rallier_started_v1;
            assert!(!started.is_empty());
            let id = started.0.unwrap().stack_item_id;
            assert_eq!(state.stack.last().unwrap(), &pending.resolving_item);
            assert_eq!(started.0.unwrap().source.controller, actor);
            let selected = state.players[actor.index()].library[0];
            let action = if take {
                Action::ChooseEffectTarget(Target::Object(selected))
            } else {
                Action::FinishEffectSelection
            };
            engine::step(&mut state, action).unwrap();
            state = restore(&restore(&state));
            let mut missing_live = restore(&state);
            missing_live
                .stack
                .retain(|item| item.v4.stack_item_id != id);
            let before = serde_json::to_vec(&missing_live).unwrap();
            assert!(mtg_kernel::effect::resume_resumable_resolution(&mut missing_live).is_err());
            assert_eq!(serde_json::to_vec(&missing_live).unwrap(), before);
            readonly(&mut missing_live, Action::Pass);
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(state.players[actor.index()].hand.contains(&selected), take);
            assert!(state.stack.is_empty() && state.engine.pending_effect.is_none());
            assert!(state.engine.event_history.iter().any(
                |event| matches!(event, CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt }
                    if receipt.stack_item_id == id && receipt.pending.is_none())
            ));
            readonly(&mut state, Action::FinishEffectSelection);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
