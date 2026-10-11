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
fn precise_meaning_authenticates_actual_printed_trigger_and_refuses_forged_facts() {
    use mtg_kernel::rules_meaning_frenzied_goblin_v1 as precise;
    let card = card_id_by_name("Frenzied Goblin").expect("true future registration required");
    let selector = precise::live_selector().unwrap();
    let record = precise::meaning(&selector, card, 0).unwrap();
    precise::validate(&record).unwrap();
    assert_eq!(
        record.target_spec,
        mtg_kernel::trigger::trigger_target_spec(card)
    );
    assert_eq!((record.target_minimum, record.target_maximum), (1, 1));
    assert!(record.target_mandatory_at_placement_before_payment);
    assert_eq!(record.payment_colored, [ManaColor::R]);
    assert_eq!(record.payment_generic, 0);
    assert!(record.payment_optional_at_resolution);
    assert_eq!(
        precise::meaning(&selector, card, 1),
        Err(precise::ErrorV1::WrongRoot)
    );
    let decoded: precise::MeaningV1 =
        serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
    assert_eq!(record, decoded);
    for field in 0..17 {
        let mut forged = record.clone();
        match field {
            0 => forged.payment_colored = vec![ManaColor::G],
            1 => forged.payment_generic = 1,
            2 => forged.target_mandatory_at_placement_before_payment = false,
            3 => forged.target_any_controller_and_any_power = false,
            4 => forged.payer_is_captured_trigger_controller = false,
            5 => forged.source_survival_required_after_capture = true,
            6 => forged.restriction_expires_at_next_cleanup_including_repeated_cleanup = false,
            7 => forged.restriction_is_rule_change_not_removable_keyword = false,
            8 => forged.illegal_sole_target_fizzles_without_payment_choice = false,
            9 => forged.source_zone = Zone::Hand,
            10 => forged.source_face_index = 1,
            11 => forged.target_minimum = 0,
            12 => forged.target_maximum = 2,
            13 => forged.recheck_exact_target_incarnation_and_full_legality_at_resolution = false,
            14 => forged.root_sha256.push('x'),
            15 => forged.target_spec = mtg_kernel::card_def::TargetSpec::ControlledCreature,
            _ => forged.recipient_control_or_ability_changes_end_restriction = true,
        }
        assert_eq!(
            precise::validate(&forged),
            Err(precise::ErrorV1::IdentityMismatch)
        );
    }
    let mut unknown = serde_json::to_value(&record).unwrap();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("execution_authority".into(), true.into());
    assert!(serde_json::from_value::<precise::MeaningV1>(unknown).is_err());
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
                assert!(state.engine.event_history.iter().any(|event|matches!(event,
                    mtg_kernel::event::CommittedEvent::FrenziedGoblinCleanupV1 {step:Step::Cleanup,ending_player,..}
                        if *ending_player==actor)));
                assert_eq!(
                    serde_json::to_vec(&restore(&state)).unwrap(),
                    serde_json::to_vec(&state).unwrap()
                );
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
                    _ => **then = mtg_kernel::effect::EffectOp::Sequence(vec![]),
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

#[test]
fn older_actual_attack_controller_does_not_authorize_current_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = fixture(actor);
        target(&mut state, actor, source, recipient);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        payment(&mut state, actor, source);
        engine::step(&mut state, Action::ChooseEffectBoolean(false)).unwrap();
        finish(&mut state);
        loop {
            let d = next(&mut state);
            if matches!(d, Decision::DeclareBlockers { .. }) {
                engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap();
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        cleanup(&mut state, actor);
        let current = actor.opponent();
        control_primitive(&mut state, source, current);
        state.objects.get_mut(source).tapped = false;
        granted_target_keyword_primitive(&mut state, source, Keywords::HASTE);
        attack(&mut state, source);
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { player, .. } if player == current)
        );
        assert!(state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event,
            mtg_kernel::event::CommittedEvent::DeclaredAttacker { source: s, controller, .. }
            if *s == source && *controller == actor)));
        let mut forged = restore(&state);
        let pending = &mut forged.engine.pending_triggers[0];
        pending.controller = actor;
        pending.source_contract.as_mut().unwrap().controller = actor;
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(
            engine::step(&mut forged, Action::ChooseTarget(Target::Object(recipient))).is_err()
        );
        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        target(&mut state, current, source, recipient);
    }
}

#[test]
fn published_payment_and_installed_authority_forgeries_refuse_readonly_after_restore() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = fixture(actor);
        target(&mut state, actor, source, recipient);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        payment(&mut state, actor, source);
        for case in 0..9 {
            let mut corrupt = restore(&state);
            match case {
                0 => corrupt.engine.pending_effect = None,
                1 => corrupt.engine.event_history.retain(|event| {
                    !matches!(
                        event,
                        CommittedEvent::FrenziedGoblinResolutionBoundaryV1 { .. }
                    )
                }),
                2 => {
                    corrupt.engine.pending_effect = None;
                    corrupt.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            CommittedEvent::FrenziedGoblinResolutionBoundaryV1 { .. }
                        )
                    });
                }
                3 => corrupt.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::FrenziedGoblinPublishedV1 { .. })
                }),
                4 => {
                    corrupt
                        .stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .frenzied_goblin_started_v1 = Default::default()
                }
                5 => {
                    corrupt
                        .stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .frenzied_goblin_publication_v1 = Default::default()
                }
                6 => {
                    corrupt
                        .engine
                        .pending_effect
                        .as_mut()
                        .unwrap()
                        .ctx
                        .controller = actor.opponent()
                }
                7 => {
                    corrupt.stack.pop().unwrap();
                }
                _ => {
                    let event = corrupt
                        .engine
                        .event_history
                        .iter()
                        .find(|event| {
                            matches!(event, CommittedEvent::FrenziedGoblinPublishedV1 { .. })
                        })
                        .unwrap()
                        .clone();
                    corrupt.engine.event_log.push(event);
                }
            }
            corrupt = restore(&corrupt);
            for action in [Action::ChooseEffectBoolean(true), Action::Pass] {
                let before = serde_json::to_vec(&corrupt).unwrap();
                assert!(engine::step(&mut corrupt, action).is_err(), "case {case}");
                assert_eq!(serde_json::to_vec(&corrupt).unwrap(), before, "case {case}");
            }
        }
        engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
        finish(&mut state);
        assert!(restricted(&state, recipient));
        for case in 0..7 {
            let mut corrupt = restore(&state);
            match case {
                0 => corrupt.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::FrenziedGoblinPaymentV1 { .. })
                }),
                1 => corrupt.engine.until_end_of_turn.retain(|effect| {
                    !matches!(effect, UntilEndOfTurnEffect::ResolvedObjectCantBlock { .. })
                }),
                2 => {
                    let duplicate = corrupt
                        .engine
                        .event_history
                        .iter()
                        .find(|event| {
                            matches!(event, CommittedEvent::FrenziedGoblinInstalledV1 { .. })
                        })
                        .unwrap()
                        .clone();
                    corrupt.engine.event_history.push(duplicate);
                }
                3 => corrupt.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::FrenziedGoblinCompletedV1 { .. })
                }),
                4 => {
                    for event in &mut corrupt.engine.event_history {
                        if let CommittedEvent::FrenziedGoblinInstalledV1 { capture } = event {
                            capture.recipient.zone_change_count += 1;
                        }
                    }
                }
                5 => {
                    for event in &mut corrupt.engine.event_history {
                        if let CommittedEvent::FrenziedGoblinCompletedV1 { outcome, .. } = event {
                            *outcome = mtg_kernel::effect::FrenziedGoblinOutcomeV1::Countered;
                        }
                    }
                }
                _ => {}
            }
            if case == 6 {
                let capture = corrupt
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::FrenziedGoblinInstalledV1 { capture } => {
                            Some(capture.clone())
                        }
                        _ => None,
                    })
                    .unwrap();
                corrupt.engine.until_end_of_turn.retain(|effect| {
                    !matches!(effect, UntilEndOfTurnEffect::ResolvedObjectCantBlock { .. })
                });
                corrupt
                    .engine
                    .event_history
                    .push(CommittedEvent::FrenziedGoblinExpiredV1 {
                        capture,
                        cleanup_history_index: u32::MAX,
                    });
                corrupt = restore(&corrupt);
            }
            let before = serde_json::to_vec(&corrupt).unwrap();
            assert!(
                engine::step(&mut corrupt, Action::Pass).is_err(),
                "installed case {case}"
            );
            assert_eq!(serde_json::to_vec(&corrupt).unwrap(), before);
        }
    }
}
