//! Future actual printed acceptance, requiring parent-owned real registration.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, EffectDuration, Layers, UntilEndOfTurnEffect};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("true future definition registration is required");
    let id = state.objects.push(GameObject {
        card_def: def,
        name: CARD_DEFS[usize::from(def)].object_name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("unsupported fixture placement"),
    }
    id
}
fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1127,
        actor,
    );
    state.step = Step::Main1;
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("missing priority");
}
fn finish(state: &mut GameState) {
    for _ in 0..30 {
        let d = next(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && matches!(d, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("resolution did not complete");
}
fn restricted(state: &GameState, target: ObjectId) -> bool {
    state.engine.until_end_of_turn.iter().any(|effect| matches!(effect,
        UntilEndOfTurnEffect::ResolvedObjectCantBlock { object_id, object_zone_change_count, .. }
        if *object_id == target && *object_zone_change_count == state.objects.get(target).zone_change_count
            && state.objects.get(target).zone == Zone::Battlefield))
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&value).unwrap(), bytes);
    value
}
fn control_primitive(state: &mut GameState, target: ObjectId, controller: PlayerId) {
    let old = state.objects.get(target).controller;
    state.players[old.index()]
        .battlefield
        .retain(|&id| id != target);
    state.players[controller.index()].battlefield.push(target);
    state.objects.get_mut(target).controller = controller;
}
fn granted_target_keyword_primitive(state: &mut GameState, target: ObjectId, keyword: Keywords) {
    let timestamp = state
        .engine
        .until_end_of_turn
        .iter()
        .filter_map(|effect| {
            if let UntilEndOfTurnEffect::ResolvedObjectCantBlock { timestamp, .. } = effect {
                Some(*timestamp)
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0)
        + 100;
    state
        .engine
        .until_end_of_turn
        .push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: target,
            object_zone_change_count: state.objects.get(target).zone_change_count,
            layer: Layers::ABILITY_ADDING,
            timestamp,
            duration: EffectDuration::EndOfTurn,
            keywords: keyword,
        });
}
fn witness(state: &mut GameState, actor: PlayerId, target: ObjectId) {
    let aura = put(state, actor, "Witness Protection", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(aura)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    finish(state);
    assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
}
fn cleanup(state: &mut GameState, ending: PlayerId) {
    for _ in 0..40 {
        let d = next(state);
        if state.active_player != ending {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("no cleanup/player flip");
}

fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Frenzied Goblin", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    priority(state, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    // Accepted physical haste grant, not an invented printed ability.
    granted_target_keyword_primitive(state, source, Keywords::HASTE);
    source
}
fn attack(state: &mut GameState, source: ObjectId) {
    for _ in 0..20 {
        let d = next(state);
        if let Decision::DeclareAttackers { eligible, .. } = d {
            assert!(eligible.contains(&source));
            engine::step(state, Action::DeclareAttackers(vec![source])).unwrap();
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("no actual attack declaration");
}
fn target(state: &mut GameState, actor: PlayerId, source: ObjectId, target: ObjectId) {
    assert!(matches!(next(state), Decision::ChooseTargets {
        player, spell, remaining: 1, can_finish: false, legal_targets,
    } if player == actor && spell == source && legal_targets.contains(&Target::Object(target))));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|item| item.source == source
        && item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility));
}
fn payment(state: &mut GameState, actor: PlayerId, source: ObjectId) {
    for _ in 0..10 {
        let d = next(state);
        if let Decision::ChooseEffectBoolean {
            player,
            source: s,
            default,
            purpose,
        } = d
        {
            assert_eq!((player, s, default), (actor, source, Some(false)));
            assert!(
                matches!(purpose, mtg_kernel::effect::EffectBooleanChoicePurpose::PayManaThen {
                player, colored, generic: 0, then,
            } if player == actor && colored == vec![ManaColor::R]
                && *then == (mtg_kernel::effect::EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 }))
            );
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("missing optional resolution payment");
}
fn fixture(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready(actor);
    let source = source(&mut state, actor);
    let target = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    attack(&mut state, source);
    (state, source, target)
}

#[test]
fn printed_metadata_exact_attack_trigger_and_no_new_catalog_ids() {
    let id = card_id_by_name("Frenzied Goblin").expect("real future registration required");
    let def = &CARD_DEFS[usize::from(id)];
    assert!(def.is_executable());
    assert_eq!(
        (def.mana_value, def.power, def.toughness),
        (1, Some(1), Some(1))
    );
    assert_eq!(def.cost.generic, 0);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Goblin, Subtype::Berserker]);
    assert_eq!(def.colors, &[ManaColor::R]);
    let triggers = mtg_kernel::trigger::triggers_for(id);
    assert_eq!(triggers.len(), 1);
    assert_eq!(
        triggers[0].condition,
        mtg_kernel::trigger::TriggerCondition::Attacks
    );
    assert_eq!(
        mtg_kernel::trigger::trigger_target_spec(id),
        mtg_kernel::card_def::TargetSpec::Creature
    );
}

#[test]
fn real_attack_targets_before_optional_payment_pay_decline_and_unpayable() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for choice in 0..3 {
            let (mut state, source, recipient) = fixture(actor);
            assert!(
                state.engine.pending_effect.is_none(),
                "no payment before targeting"
            );
            let mut pending_replay = restore(&state);
            target(&mut state, actor, source, recipient);
            target(&mut pending_replay, actor, source, recipient);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&pending_replay).unwrap()
            );
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] =
                u8::from(choice != 2);
            // Prevent an untapped Mountain from making the negative payable.
            for &id in &state.players[actor.index()].battlefield.clone() {
                if CARD_DEFS[usize::from(state.objects.get(id).card_def)]
                    .types
                    .contains(&CardType::Land)
                {
                    state.objects.get_mut(id).tapped = true;
                }
            }
            if choice == 2 {
                finish(&mut state);
                assert!(!restricted(&state, recipient));
                assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
                continue;
            }
            payment(&mut state, actor, source);
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ChooseEffectBoolean(choice == 0)).unwrap();
                // Payment is executed by the resumed driver, not the answer.
                finish(current);
                assert_eq!(restricted(current, recipient), choice == 0);
                assert_eq!(
                    current.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
                    u8::from(choice != 0)
                );
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            if choice == 0 {
                // Actual blocker decision and readonly refusal use the same restriction.
                let d = loop {
                    let d = next(&mut state);
                    if matches!(d, Decision::DeclareBlockers { .. }) {
                        break d;
                    }
                    assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                    engine::step(&mut state, Action::Pass).unwrap();
                };
                assert!(
                    matches!(d, Decision::DeclareBlockers { legal_blockers, .. } if legal_blockers.iter().all(|(_, blockers)| !blockers.contains(&recipient)))
                );
                let before = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(
                    &mut state,
                    Action::DeclareBlockers(vec![(recipient, source)])
                )
                .is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap();
                cleanup(&mut state, actor);
                assert!(!restricted(&state, recipient));
            }
        }
    }
}

#[test]
fn source_owner_control_departure_and_recipient_control_do_not_change_payer() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..4 {
            let (mut state, source, recipient) = fixture(actor);
            target(&mut state, actor, source, recipient);
            if mode == 0 {
                control_primitive(&mut state, source, actor.opponent());
            }
            if mode == 1 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
            }
            if mode == 2 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
            }
            if mode == 3 {
                control_primitive(&mut state, recipient, actor);
            }
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[actor.opponent().index()].mana_pool[ManaColor::R.pool_index()] = 2;
            payment(&mut state, actor, source);
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ChooseEffectBoolean(true)).unwrap();
                finish(current);
                assert!(restricted(current, recipient));
                assert_eq!(
                    current.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
                    0
                );
                assert_eq!(
                    current.players[actor.opponent().index()].mana_pool[ManaColor::R.pool_index()],
                    2
                );
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn target_departure_reentry_hexproof_and_protection_fizzle_before_payment() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..4 {
            let (mut state, source, recipient) = fixture(actor);
            target(&mut state, actor, source, recipient);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            match mode {
                0 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(recipient, Zone::Graveyard),
                    );
                }
                1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(recipient, Zone::Graveyard),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(recipient, Zone::Battlefield),
                    );
                }
                2 => granted_target_keyword_primitive(&mut state, recipient, Keywords::HEXPROOF),
                _ => granted_target_keyword_primitive(
                    &mut state,
                    recipient,
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                ),
            }
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert!(!restricted(&state, recipient));
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
                1
            );
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn protection_at_placement_wrong_target_and_payment_forgeries_are_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = fixture(actor);
        granted_target_keyword_primitive(
            &mut state,
            recipient,
            Keywords::PROTECTION_FROM_MONOCOLORED,
        );
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if !legal_targets.contains(&Target::Object(recipient)) && legal_targets.contains(&Target::Object(source)))
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert!(engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent()))
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        target(&mut state, actor, source, source);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        payment(&mut state, actor, source);
        for mode in 0..4 {
            let mut forged = restore(&state);
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            if mode < 3 {
                let Some(mtg_kernel::effect::PendingEffectChoice::ChooseBoolean {
                    player,
                    purpose:
                        mtg_kernel::effect::EffectBooleanChoicePurpose::PayManaThen {
                            player: payer,
                            colored,
                            then,
                            ..
                        },
                    ..
                }) = pending.choice.as_mut()
                else {
                    panic!("real mana prompt");
                };
                match mode {
                    0 => {
                        *player = actor.opponent();
                        *payer = actor.opponent();
                    }
                    1 => *colored = vec![ManaColor::G],
                    _ => *then = Box::new(mtg_kernel::effect::EffectOp::Sequence(vec![])),
                }
            } else {
                pending.choice = None;
                pending.frames = vec![mtg_kernel::effect::EffectFrame::Program {
                    op: mtg_kernel::effect::EffectOp::PreventTargetBlockingUntilEndOfTurn {
                        target_index: 0,
                    },
                    path: vec![1],
                }];
                pending.answered_choice_guard = None;
            }
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());
            assert!(engine::step(&mut forged, Action::ChooseEffectBoolean(true)).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
            assert!(engine::step(&mut forged, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
    }
}

#[test]
fn actual_witness_suppression_and_empty_declaration_do_not_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for suppressed in [false, true] {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            if suppressed {
                witness(&mut state, actor, source);
            }
            // Attack eligibility is independent of the presence of the attack ability.
            granted_target_keyword_primitive(&mut state, source, Keywords::HASTE);
            loop {
                let d = next(&mut state);
                if let Decision::DeclareAttackers { eligible, .. } = d {
                    assert!(eligible.contains(&source));
                    engine::step(
                        &mut state,
                        Action::DeclareAttackers(if suppressed { vec![source] } else { vec![] }),
                    )
                    .unwrap();
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(&mut state, Action::Pass).unwrap();
            }
            let d = next(&mut state);
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            assert!(state.engine.pending_triggers.is_empty());
            assert!(state.stack.is_empty());
            assert!(state.engine.pending_effect.is_none());
        }
    }
}

#[test]
fn physical_owner_differs_from_declaring_controller_and_source_capture_forgery_refuses() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        // Registered-definition physical control primitive, then actual attack.
        let mut state = ready(actor);
        let source = put(
            &mut state,
            actor.opponent(),
            "Frenzied Goblin",
            Zone::Battlefield,
        );
        control_primitive(&mut state, source, actor);
        granted_target_keyword_primitive(&mut state, source, Keywords::HASTE);
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        attack(&mut state, source);
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { player, .. } if player == actor)
        );
        for mode in 0..3 {
            let mut forged = restore(&state);
            let pending = &mut forged.engine.pending_triggers[0];
            let source_contract = pending.source_contract.as_mut().unwrap();
            match mode {
                0 => {
                    pending.controller = actor.opponent();
                    source_contract.controller = actor.opponent();
                }
                1 => source_contract.zone_change_count += 1,
                _ => {
                    source_contract.owner = actor;
                }
            }
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(
                engine::step(&mut forged, Action::ChooseTarget(Target::Object(recipient))).is_err()
            );
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
        target(&mut state, actor, source, recipient);
        assert_eq!(state.objects.get(source).owner, actor.opponent());
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        payment(&mut state, actor, source);
        engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
        let mut accepted_answer = restore(&state);
        finish(&mut state);
        finish(&mut accepted_answer);
        assert!(restricted(&state, recipient));
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&accepted_answer).unwrap()
        );
    }
}
