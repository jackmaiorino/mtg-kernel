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
        UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 { object_id, object_zone_change_count, .. }
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
            if let UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 { timestamp, .. } = effect {
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
    let source = put(state, actor, "Vampire Gourmand", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
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

fn selection(state: &mut GameState, actor: PlayerId, source: ObjectId) -> Vec<Target> {
    for _ in 0..12 {
        let d = next(state);
        if let Decision::ChooseEffectTargets {
            player,
            source: s,
            min_targets,
            max_targets,
            selected_count,
            legal_targets,
            can_finish,
        } = d
        {
            assert_eq!(
                (player, s, min_targets, selected_count),
                (actor, source, 0, 0)
            );
            assert!(max_targets <= 1 && can_finish);
            return legal_targets;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
        for item in &state.stack {
            if item.source == source
                && item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility
            {
                assert!(item.targets.is_empty() && item.v4.target_contracts.is_empty());
                assert!(item.v4.target_spec.is_none());
                assert!(item.v4.vampire_gourmand_started_v1.is_empty());
            }
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("no optional resolution sacrifice");
}
fn fixture(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready(actor);
    let source = source(&mut state, actor);
    let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    attack(&mut state, source);
    (state, source, other)
}

#[test]
fn actual_future_printed_metadata_and_untargeted_attack_root() {
    let id = card_id_by_name("Vampire Gourmand").expect("actual parent registration required");
    let def = &CARD_DEFS[usize::from(id)];
    assert!(def.is_executable());
    assert_eq!(
        (def.mana_value, def.power, def.toughness),
        (2, Some(2), Some(2))
    );
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert_eq!(def.colors, &[ManaColor::B]);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Vampire]);
    let abilities = mtg_kernel::trigger::triggers_for(id);
    assert_eq!(abilities.len(), 1);
    assert_eq!(
        abilities[0].condition,
        mtg_kernel::trigger::TriggerCondition::Attacks
    );
    assert_eq!(
        mtg_kernel::trigger::trigger_target_spec(id),
        mtg_kernel::card_def::TargetSpec::None
    );
}

#[test]
fn real_cast_attack_optional_sacrifice_draw_then_unblockable_restore_and_blocker_refusal() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let (mut state, source, other) = fixture(actor);
            let blocker = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let top = state.players[actor.index()].library[0];
            let hand = state.players[actor.index()].hand.len();
            assert!(selection(&mut state, actor, source).contains(&Target::Object(other)));
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                let before = serde_json::to_vec(current).unwrap();
                assert!(
                    engine::step(current, Action::ChooseEffectTarget(Target::Object(source)))
                        .is_err()
                );
                assert_eq!(serde_json::to_vec(current).unwrap(), before);
                assert!(
                    engine::step(current, Action::ChooseEffectTarget(Target::Object(blocker)))
                        .is_err()
                );
                assert_eq!(serde_json::to_vec(current).unwrap(), before);
                if pay {
                    engine::step(current, Action::ChooseEffectTarget(Target::Object(other)))
                        .unwrap();
                } else {
                    engine::step(current, Action::FinishEffectSelection).unwrap();
                }
                // The selection answer finalizes the choice; the resumed driver pays it.
                assert_eq!(current.objects.get(other).zone, Zone::Battlefield);
                finish(current);
                assert_eq!(
                    current.objects.get(other).zone,
                    if pay {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    current.players[actor.index()].hand.len(),
                    hand + usize::from(pay)
                );
                assert_eq!(restricted(current, source), pay);
                if pay {
                    assert!(current.players[actor.index()].hand.contains(&top));
                    assert!(current
                        .known_hand_cards(actor.opponent(), actor)
                        .iter()
                        .all(|k| k.object != top));
                    assert!(!engine::has_effective_keyword(
                        current,
                        source,
                        Keywords::CANT_BE_BLOCKED
                    ));
                    assert!(
                        mtg_kernel::rl::observe_policy_v8(
                            current,
                            &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                            actor,
                            0,
                            0,
                            0,
                            1
                        )
                        .is_err(),
                        "frozen transport must refuse new active evasion"
                    );
                }
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            loop {
                let d = next(&mut state);
                if let Decision::DeclareBlockers { legal_blockers, .. } = d {
                    assert_eq!(
                        legal_blockers.iter().any(|(_, bs)| bs.contains(&blocker)),
                        !pay
                    );
                    if pay {
                        let before = serde_json::to_vec(&state).unwrap();
                        assert!(engine::step(
                            &mut state,
                            Action::DeclareBlockers(vec![(blocker, source)])
                        )
                        .is_err());
                        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                    }
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap();
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(&mut state, Action::Pass).unwrap();
            }
            cleanup(&mut state, actor);
            assert!(!restricted(&state, source));
        }
    }
}

#[test]
fn empty_other_creature_pool_declines_without_target_or_draw() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let hand = state.players[actor.index()].hand.clone();
        attack(&mut state, source);
        assert!(selection(&mut state, actor, source).is_empty());
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::FinishEffectSelection).unwrap();
            finish(current);
            assert_eq!(current.players[actor.index()].hand, hand);
            assert!(!restricted(current, source));
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn captured_controller_draws_and_cost_uses_current_own_creatures_after_source_response() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let (mut state, source, other) = fixture(actor);
            let top = state.players[actor.index()].library[0];
            let opposing_hand = state.players[actor.opponent().index()].hand.clone();
            // These accepted physical response primitives run before resolution.
            // They do not fabricate a program or a printed producer.
            match mode {
                0 => control_primitive(&mut state, source, actor.opponent()),
                1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Graveyard),
                    );
                }
                _ => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Graveyard),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
            }
            let legal = selection(&mut state, actor, source);
            assert!(legal.contains(&Target::Object(other)));
            if mode == 2 {
                assert!(
                    legal.contains(&Target::Object(source)),
                    "returned source is another incarnation"
                );
            }
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ChooseEffectTarget(Target::Object(other))).unwrap();
                finish(current);
                assert!(current.players[actor.index()].hand.contains(&top));
                assert_eq!(
                    current.players[actor.opponent().index()].hand,
                    opposing_hand
                );
                assert_eq!(restricted(current, source), mode == 0);
                if mode != 0 {
                    let own = mtg_kernel::rl::observe_v1(current, actor, 0).unwrap();
                    let opponent =
                        mtg_kernel::rl::observe_v1(current, actor.opponent(), 0).unwrap();
                    assert!(own.own_hand.iter().any(|c| c.stable.arena_id == top.0));
                    assert!(opponent.own_hand.iter().all(|c| c.stable.arena_id != top.0));
                    assert!(current
                        .known_hand_cards(actor.opponent(), actor)
                        .iter()
                        .all(|c| c.object != top));
                }
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn sacrifice_is_not_targeted_and_stolen_physical_owner_gets_its_graveyard_card() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let stolen = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        control_primitive(&mut state, stolen, actor);
        granted_target_keyword_primitive(&mut state, stolen, Keywords::HEXPROOF);
        granted_target_keyword_primitive(&mut state, stolen, Keywords::PROTECTION_FROM_MONOCOLORED);
        attack(&mut state, source);
        let legal = selection(&mut state, actor, source);
        assert!(legal.contains(&Target::Object(stolen)));
        let start = state.engine.event_history.len();
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(stolen)),
        )
        .unwrap();
        let mut replay = restore(&state);
        finish(&mut state);
        finish(&mut replay);
        assert!(state.players[actor.opponent().index()]
            .graveyard
            .contains(&stolen));
        assert!(!state.players[actor.index()].graveyard.contains(&stolen));
        assert!(restricted(&state, source));
        assert!(state.engine.event_history[start..]
            .iter()
            .any(|e| matches!(e,
            mtg_kernel::event::CommittedEvent::Sacrificed { object, controller_before, .. }
            if *object == stolen && *controller_before == actor)));
        assert!(!state.engine.event_history[start..]
            .iter()
            .any(|e| matches!(e,
            mtg_kernel::event::CommittedEvent::Targeted { target, .. } if *target == stolen)));
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        // A real Witness cast after resolution removes abilities, not this rule.
        state.step = Step::Main2;
        witness(&mut state, actor, source);
        assert!(restricted(&state, source));
        assert!(!engine::has_effective_keyword(
            &state,
            source,
            Keywords::CANT_BE_BLOCKED
        ));
    }
}

#[test]
fn restored_selection_missing_stage_root_restart_controller_and_stale_answers_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, other) = fixture(actor);
        selection(&mut state, actor, source);
        for mode in 0..10 {
            let mut forged = restore(&state);
            match mode {
                0 => {
                    let p = forged.engine.pending_effect.as_mut().unwrap();
                    p.choice = None;
                    p.frames.clear();
                    p.answered_choice_guard = None;
                }
                1 => {
                    let p = forged.engine.pending_effect.as_mut().unwrap();
                    p.choice = None;
                    p.frames = vec![mtg_kernel::effect::EffectFrame::Program {
                        op: p.resolving_item.inline_effect.clone().unwrap(),
                        path: vec![],
                    }];
                    p.answered_choice_guard = None;
                }
                2 => {
                    forged.engine.pending_effect = None;
                }
                3 => {
                    let p = forged.engine.pending_effect.as_mut().unwrap();
                    p.ctx.controller = actor.opponent();
                    p.resolving_item.controller = actor.opponent();
                    p.ctx.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
                    p.resolving_item
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .controller = actor.opponent();
                    let stack = forged
                        .stack
                        .iter_mut()
                        .find(|s| s.v4.stack_item_id == p.resolving_item.v4.stack_item_id)
                        .unwrap();
                    *stack = p.resolving_item.clone();
                }
                4 => {
                    event::propose_and_commit(
                        &mut forged,
                        ProposedEvent::zone_change(other, Zone::Graveyard),
                    );
                    event::propose_and_commit(
                        &mut forged,
                        ProposedEvent::zone_change(other, Zone::Battlefield),
                    );
                }
                5 => control_primitive(&mut forged, other, actor.opponent()),
                6 => {
                    // Erase both mutable continuation and every progress
                    // receipt. The genuine live stack start guard remains.
                    forged.engine.pending_effect = None;
                    forged.engine.event_history.retain(|event| !matches!(event,
                        mtg_kernel::event::CommittedEvent::VampireGourmandResolutionBoundaryV1 { .. }));
                }
                7 => {
                    let id = forged
                        .engine
                        .pending_effect
                        .as_ref()
                        .unwrap()
                        .resolving_item
                        .v4
                        .stack_item_id;
                    forged
                        .stack
                        .iter_mut()
                        .find(|item| item.v4.stack_item_id == id)
                        .unwrap()
                        .v4
                        .vampire_gourmand_started_v1
                        .0 = None;
                }
                8 => {
                    let id = forged
                        .engine
                        .pending_effect
                        .as_ref()
                        .unwrap()
                        .resolving_item
                        .v4
                        .stack_item_id;
                    forged
                        .stack
                        .iter_mut()
                        .find(|item| item.v4.stack_item_id == id)
                        .unwrap()
                        .v4
                        .vampire_gourmand_started_v1
                        .0
                        .as_mut()
                        .unwrap()
                        .first_receipt_history_index += 1;
                }
                _ => {
                    let id = forged
                        .engine
                        .pending_effect
                        .as_ref()
                        .unwrap()
                        .resolving_item
                        .v4
                        .stack_item_id;
                    forged
                        .stack
                        .iter_mut()
                        .find(|item| item.v4.stack_item_id == id)
                        .unwrap()
                        .v4
                        .vampire_gourmand_started_v1
                        .0
                        .as_mut()
                        .unwrap()
                        .source
                        .controller = actor.opponent();
                }
            }
            forged = restore(&restore(&forged));
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());
            assert!(engine::step(
                &mut forged,
                Action::ChooseEffectTarget(Target::Object(other))
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
            assert!(engine::step(&mut forged, Action::FinishEffectSelection).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
            assert!(engine::step(&mut forged, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(other)),
        )
        .unwrap();
        finish(&mut state);
        assert!(restricted(&state, source));
    }
}

#[test]
fn witness_before_real_attack_and_empty_declaration_have_no_optional_sacrifice() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for suppressed in [false, true] {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            if suppressed {
                witness(&mut state, actor, source);
            }
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
            assert!(state.stack.is_empty() && state.engine.pending_triggers.is_empty());
            assert!(state.engine.pending_effect.is_none());
            assert!(!restricted(&state, source));
        }
    }
}

#[test]
fn real_answer_resumes_only_with_engine_removed_item_ownership_after_repeated_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let (mut state, source, other) = fixture(actor);
            selection(&mut state, actor, source);
            let started = state
                .engine
                .pending_effect
                .as_ref()
                .unwrap()
                .resolving_item
                .v4
                .vampire_gourmand_started_v1;
            assert!(!started.is_empty());
            let action = if pay {
                Action::ChooseEffectTarget(Target::Object(other))
            } else {
                Action::FinishEffectSelection
            };
            engine::step(&mut state, action).unwrap();
            state = restore(&restore(&state));
            let mut missing_live = restore(&state);
            let id = started.0.unwrap().stack_item_id;
            missing_live
                .stack
                .retain(|item| item.v4.stack_item_id != id);
            let before = serde_json::to_vec(&missing_live).unwrap();
            assert!(mtg_kernel::effect::resume_resumable_resolution(&mut missing_live).is_err());
            assert_eq!(serde_json::to_vec(&missing_live).unwrap(), before);
            assert!(engine::step(&mut missing_live, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&missing_live).unwrap(), before);
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert_eq!(restricted(&state, source), pay);
            assert_eq!(
                state.objects.get(other).zone,
                if pay {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

fn refused_cost_state(state: &GameState) {
    let bytes = serde_json::to_vec(state).unwrap();
    for _ in 0..2 {
        let mut bad: GameState = serde_json::from_slice(&bytes).unwrap();
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        assert!(matches!(
            engine::advance_until_decision(&mut bad),
            Decision::Halted { .. }
        ));
        bad.engine.halted = None;
        assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
    }
}

#[test]
fn manufactured_closed_boundary_cannot_skip_genuine_accepted_cost() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, other) = fixture(actor);
        selection(&mut state, actor, source);
        for paid in [false, true] {
            let mut selected = restore(&state);
            engine::step(
                &mut selected,
                if paid {
                    Action::ChooseEffectTarget(Target::Object(other))
                } else {
                    Action::FinishEffectSelection
                },
            )
            .unwrap();
            let original_journal = selected.engine.event_history.clone();
            let mut bad = restore(&selected);
            let mut closed = bad
                .engine
                .event_history
                .iter()
                .rev()
                .find_map(|e| {
                    if let CommittedEvent::VampireGourmandResolutionBoundaryV1 { receipt } = e {
                        Some(receipt.clone())
                    } else {
                        None
                    }
                })
                .unwrap();
            let id = closed.stack_item_id;
            closed.previous_history_index = Some(closed.history_index);
            closed.history_index = bad.engine.event_history.len().try_into().unwrap();
            closed.ordinal += 1;
            closed.pending = None;
            bad.engine.pending_effect = None;
            bad.stack.retain(|i| i.v4.stack_item_id != id);
            bad.engine
                .event_history
                .push(CommittedEvent::VampireGourmandResolutionBoundaryV1 { receipt: closed });
            assert_eq!(
                &bad.engine.event_history[..original_journal.len()],
                &original_journal
            );
            refused_cost_state(&bad);
            assert_eq!(bad.objects.get(other).zone, Zone::Battlefield);
        }
    }
}

#[test]
fn actual_paid_and_declined_operations_restore_and_reject_missing_draw_or_replayed_row() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for paid in [false, true] {
            let (mut state, source, other) = fixture(actor);
            selection(&mut state, actor, source);
            engine::step(
                &mut state,
                if paid {
                    Action::ChooseEffectTarget(Target::Object(other))
                } else {
                    Action::FinishEffectSelection
                },
            )
            .unwrap();
            let saved = state
                .engine
                .pending_effect
                .as_ref()
                .unwrap()
                .resolving_item
                .clone();
            let hand_before = state.players[actor.index()].hand.len();
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert_eq!(
                state.players[actor.index()].hand.len(),
                hand_before + usize::from(paid)
            );
            assert_eq!(
                state.objects.get(other).zone,
                if paid {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(restricted(&state, source), paid);
            for case in 0..4 {
                let mut bad = restore(&state);
                match case {
                    0 => bad.stack.push(saved.clone()),
                    1 => bad.engine.event_history.retain(|e| {
                        !matches!(e, CommittedEvent::VampireGourmandCostOperationV2 { .. })
                    }),
                    2 => {
                        let operation = bad
                            .engine
                            .event_history
                            .iter()
                            .find(|e| {
                                matches!(e, CommittedEvent::VampireGourmandCostOperationV2 { .. })
                            })
                            .unwrap()
                            .clone();
                        bad.engine.event_history.push(operation);
                    }
                    _ if paid => {
                        let CommittedEvent::VampireGourmandCostOperationV2 { operation } = bad
                            .engine
                            .event_history
                            .iter_mut()
                            .find(|e| {
                                matches!(e, CommittedEvent::VampireGourmandCostOperationV2 { .. })
                            })
                            .unwrap()
                        else {
                            unreachable!()
                        };
                        operation.result.draw_history_index = None;
                    }
                    _ => {
                        let CommittedEvent::VampireGourmandCostOperationV2 { operation } = bad
                            .engine
                            .event_history
                            .iter_mut()
                            .find(|e| {
                                matches!(e, CommittedEvent::VampireGourmandCostOperationV2 { .. })
                            })
                            .unwrap()
                        else {
                            unreachable!()
                        };
                        operation.result.draw_history_index = Some(0);
                    }
                }
                refused_cost_state(&bad);
            }
        }
    }
}

#[test]
fn installed_genuine_rule_requires_actual_cleanup_expiry_and_cannot_resurrect() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, other) = fixture(actor);
        selection(&mut state, actor, source);
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(other)),
        )
        .unwrap();
        finish(&mut state);
        let marker = state.engine.until_end_of_turn.iter().find(|e| matches!(e,
            UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 { object_id, .. } if *object_id == source)).unwrap().clone();
        let operation = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .find_map(|(at, e)| {
                matches!(e, CommittedEvent::VampireGourmandCostOperationV2 { .. })
                    .then_some(at as u32)
            })
            .unwrap();
        for case in 0..3 {
            let mut bad = restore(&state);
            match case {
                0 => bad.engine.until_end_of_turn.retain(|e| *e != marker),
                1 => bad.engine.until_end_of_turn.push(marker.clone()),
                _ => {
                    bad.engine.until_end_of_turn.retain(|e| *e != marker);
                    bad.engine
                        .event_history
                        .push(CommittedEvent::VampireGourmandExpiredV2 {
                            operation_history_index: operation,
                            cleanup_history_index: operation,
                        });
                }
            }
            refused_cost_state(&bad);
        }
        for _ in 0..30 {
            match next(&mut state) {
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap();
                    break;
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual blocker transition {other:?}"),
            }
        }
        let mut replay = restore(&state);
        cleanup(&mut state, actor);
        cleanup(&mut replay, actor);
        assert!(!restricted(&state, source));
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert!(state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, CommittedEvent::VampireGourmandCleanupV2 { .. })));
        for case in 0..3 {
            let mut bad = restore(&state);
            match case {
                0 => bad.engine.until_end_of_turn.push(marker.clone()),
                1 => bad
                    .engine
                    .event_history
                    .retain(|e| !matches!(e, CommittedEvent::VampireGourmandExpiredV2 { .. })),
                _ => {
                    let expired = bad
                        .engine
                        .event_history
                        .iter()
                        .find(|e| matches!(e, CommittedEvent::VampireGourmandExpiredV2 { .. }))
                        .unwrap()
                        .clone();
                    bad.engine.event_history.push(expired);
                }
            }
            refused_cost_state(&bad);
        }
    }
}
