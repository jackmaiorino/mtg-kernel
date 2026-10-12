//! Future actual printed fixtures require true registration, not inline roots.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Subtype, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{EffectCond, EffectOp};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ActiveReplacement, ProposedEvent, ReplacementEffectKind};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name)
        .unwrap_or_else(|| panic!("true printed registration required: {name}"));
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        Zone::Hand => state.players[owner.index()].hand.push(id),
        _ => panic!("fixture placement supports hand or battlefield"),
    }
    id
}
fn ready(actor: PlayerId) -> GameState {
    // The actual definition is in the physical pool before any life loss.
    let mut cards = vec![card_id_by_name("Forest").unwrap(); 40];
    cards[0] = card_id_by_name("Stromkirk Bloodthief").expect("true printed registration required");
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &cards,
        &cards,
        |id| CARD_DEFS[id as usize].name.into(),
        11009,
        actor,
    );
    state.step = Step::Main2;
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn settle(state: &mut GameState) {
    for _ in 0..40 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected Bloodthief stack resolution {other:?}"),
        }
    }
    panic!("Bloodthief stack did not settle");
}
fn cast_source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Stromkirk Bloodthief", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == source && item.kind == StackItemKind::Spell));
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    source
}
fn end_targets(state: &mut GameState) -> Option<Vec<Target>> {
    for _ in 0..30 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => return Some(legal_targets),
            Decision::CastSpellOrPass { .. }
                if state.step == Step::End
                    && state.stack.is_empty()
                    && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected actual end-step decision {other:?}"),
        }
    }
    panic!("did not reach real end step");
}
fn announce(state: &mut GameState, source: ObjectId, target: ObjectId, actor: PlayerId) {
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    let item = state
        .stack
        .iter()
        .find(|item| item.source == source && item.kind == StackItemKind::TriggeredAbility)
        .unwrap();
    assert_eq!(item.controller, actor);
    assert_eq!(item.targets, vec![Target::Object(target)]);
    assert_eq!(item.v4.target_contracts.len(), 1);
    assert_eq!(item.v4.ability_source_contract.unwrap().controller, actor);
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
fn change_controller(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    // Genuine registered printed control spell, with the real flash permission.
    put(state, controller, "High Fae Trickster", Zone::Battlefield);
    let spell = put(state, controller, "Involuntary Employment", Zone::Hand);
    state.players[controller.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    state.players[controller.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    for attempt in 0..20 {
        match next(state) {
            Decision::CastSpellOrPass {
                player,
                castable_spells,
                ..
            } if player == controller => {
                assert!(castable_spells.contains(&spell));
                engine::step(state, Action::CastSpell(spell)).unwrap();
                break;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("control response {other:?}"),
        }
        assert!(attempt < 19);
    }
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(object)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(object))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|i| i.source == spell));
    for attempt in 0..30 {
        if !state.stack.iter().any(|i| i.source == spell) {
            break;
        }
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
        assert!(attempt < 29);
    }
    assert_eq!(state.objects.get(object).controller, controller);
}
fn witness(state: &mut GameState, actor: PlayerId, target: ObjectId) {
    let aura = put(state, actor, "Witness Protection", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&aura))
    );
    engine::step(state, Action::CastSpell(aura)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    settle(state);
    assert!(!engine::has_effective_subtype(
        state,
        target,
        Subtype::Vampire
    ));
}
#[test]
fn printed_definition_and_root_are_exact() {
    let id = card_id_by_name("Stromkirk Bloodthief").expect("true printed registration required");
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!((def.power, def.toughness), (Some(2), Some(2)));
    assert!(def.subtypes.contains(&Subtype::Vampire));
    assert!(def.subtypes.contains(&Subtype::Rogue));
    assert_eq!(def.cost.generic, 2);
    assert_eq!(def.target_spec, TargetSpec::None);
    let triggers = mtg_kernel::trigger::triggers_for(id);
    assert_eq!(triggers.len(), 1);
    assert_eq!(
        triggers[0].condition,
        mtg_kernel::trigger::TriggerCondition::BeginningControllerEndStepIfOpponentLostLife
    );
    let root = (triggers[0].effect)();
    assert!(matches!(
        &root,
        EffectOp::Conditional {
            cond: EffectCond::OpponentLostLifeThisTurn,
            ..
        }
    ));
    assert_eq!(
        mtg_kernel::trigger::target_spec_for_trigger(id, &root),
        Some(TargetSpec::ControlledPermanentWithAnySubtype(
            [Subtype::Vampire; 4]
        ))
    );
}
#[test]
fn both_seat_actual_end_step_distinguishes_loss_payment_damage_zero_prevention_and_seat() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..7 {
            let mut state = ready(actor);
            let damage_source = state.players[actor.index()].battlefield[0];
            // Commit before the real Bloodthief enters; history must survive collection.
            match case {
                0 | 6 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::life_loss(actor.opponent(), 2),
                ),
                1 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::life_payment(actor.opponent(), 2),
                ),
                2 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::damage(damage_source, Target::Player(actor.opponent()), 2),
                ),
                3 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::life_loss(actor.opponent(), 0),
                ),
                4 => event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor, 2)),
                5 => {
                    // Existing typed prevention primitive, not a fabricated printed root.
                    state.engine.active_replacements.push(ActiveReplacement {
                        id: 11009,
                        source: damage_source,
                        kind: ReplacementEffectKind::PreventNextDamage {
                            target: Target::Player(actor.opponent()),
                            remaining: 2,
                        },
                    });
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::damage(damage_source, Target::Player(actor.opponent()), 2),
                    );
                }
                _ => unreachable!(),
            }
            if case == 6 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::life_gain(actor.opponent(), 2),
                );
            }
            let source = cast_source(&mut state, actor);
            let mut before_end = restore(&state);
            let menu = end_targets(&mut state);
            assert_eq!(menu, end_targets(&mut before_end));
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&before_end).unwrap()
            );
            let expected = matches!(case, 0 | 1 | 2 | 6);
            assert_eq!(menu.is_some(), expected);
            if expected {
                assert!(menu.unwrap().contains(&Target::Object(source)));
                let mut pending = restore(&state);
                announce(&mut state, source, source, actor);
                announce(&mut pending, source, source, actor);
                let mut announced = restore(&state);
                for branch in [&mut state, &mut pending, &mut announced] {
                    settle(branch);
                    assert_eq!(branch.objects.get(source).counters.plus1_plus1, 1);
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&announced).unwrap()
                );
            } else {
                assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
            }
        }
    }
}
#[test]
fn target_vampire_controller_filter_readonly_refusal_and_captured_source_controller() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        let vampire = put(&mut state, actor, "Vampire Soulcaller", Zone::Battlefield);
        let foreign = put(
            &mut state,
            actor.opponent(),
            "Vampire Soulcaller",
            Zone::Battlefield,
        );
        let nonvampire = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        let menu = end_targets(&mut state).unwrap();
        assert!(menu.contains(&Target::Object(source)) && menu.contains(&Target::Object(vampire)));
        assert!(
            !menu.contains(&Target::Object(foreign)) && !menu.contains(&Target::Object(nonvampire))
        );
        let before = serde_json::to_vec(&state).unwrap();
        for target in [foreign, nonvampire] {
            assert!(
                engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).is_err()
            );
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
        }
        announce(&mut state, source, vampire, actor);
        change_controller(&mut state, source, actor.opponent());
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            settle(branch);
            assert_eq!(branch.objects.get(vampire).counters.plus1_plus1, 1);
            assert_eq!(branch.objects.get(source).counters.plus1_plus1, 0);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
#[test]
fn opponent_controlled_bloodthief_does_not_trigger_at_the_wrong_players_end_step() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        change_controller(&mut state, source, actor.opponent());
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor, 1));
        assert!(end_targets(&mut state).is_none());
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
        restore(&state);
    }
}
#[test]
fn actual_witness_suppresses_source_and_target_vampire_identity() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for suppress_source in [true, false] {
            let mut state = ready(actor);
            let source = cast_source(&mut state, actor);
            let vampire = put(&mut state, actor, "Vampire Soulcaller", Zone::Battlefield);
            witness(
                &mut state,
                actor,
                if suppress_source { source } else { vampire },
            );
            event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
            let menu = end_targets(&mut state);
            if suppress_source {
                assert!(menu.is_none());
            } else {
                let menu = menu.unwrap();
                assert!(menu.contains(&Target::Object(source)));
                assert!(!menu.contains(&Target::Object(vampire)));
                announce(&mut state, source, source, actor);
                settle(&mut state);
                assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
            }
        }
    }
}
#[test]
fn restored_illegal_incarnation_or_controller_rechecks_without_counter() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for stale in [false, true] {
            let mut state = ready(actor);
            let source = cast_source(&mut state, actor);
            let target = put(&mut state, actor, "Vampire Soulcaller", Zone::Battlefield);
            event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
            end_targets(&mut state).unwrap();
            announce(&mut state, source, target, actor);
            if stale {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Battlefield),
                );
                // Soulcaller's new ETB is genuinely pending; complete its graveyard target
                // only if available. Empty graveyards leave no target and no prompt.
            } else {
                change_controller(&mut state, target, actor.opponent());
            }
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                settle(branch);
                assert_eq!(branch.objects.get(target).counters.plus1_plus1, 0);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
#[test]
fn forged_ledger_or_pending_definition_authority_halts_before_grant() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        end_targets(&mut state).unwrap();
        for kind in 0..5 {
            let mut forged = restore(&state);
            match kind {
                0 => forged.life_loss_turn_v1 = None,
                1 => {
                    forged.life_loss_turn_v1.as_mut().unwrap().players[actor.opponent().index()] =
                        false
                }
                2 => {
                    forged
                        .engine
                        .pending_triggers
                        .first_mut()
                        .unwrap()
                        .controller = actor.opponent()
                }
                3 => {
                    forged
                        .engine
                        .pending_triggers
                        .first_mut()
                        .unwrap()
                        .target_spec = TargetSpec::ControlledCreature
                }
                4 => {
                    forged.engine.pending_triggers.first_mut().unwrap().effect =
                        EffectOp::Sequence(vec![])
                }
                _ => unreachable!(),
            }
            assert!(matches!(
                engine::advance_until_decision(&mut forged),
                Decision::Halted { .. }
            ));
            assert_eq!(forged.objects.get(source).counters.plus1_plus1, 0);
        }
    }
}
#[test]
fn actual_player_turn_flip_clears_life_loss_even_when_round_number_does_not_change() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        end_targets(&mut state).unwrap();
        announce(&mut state, source, source, actor);
        settle(&mut state);
        let old_round = state.turn;
        for _ in 0..40 {
            if state.active_player != actor {
                break;
            }
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.active_player, actor.opponent());
        assert_eq!(state.turn, old_round);
        assert!(!state.player_lost_life_this_turn_v1(actor.opponent()));
        assert!(!state.player_lost_life_this_turn_v1(actor));
        let ledger = state.life_loss_turn_v1.unwrap();
        assert_eq!(ledger.active_player, actor.opponent());
        assert_eq!(ledger.players, [false; 2]);
        restore(&state);
    }
}

#[test]
fn precise_root_regenerates_every_fact_and_refuses_forged_meaning() {
    use mtg_kernel::rules_meaning_stromkirk_v1 as meaning;
    let selector = meaning::live_selector().unwrap();
    let id = card_id_by_name("Stromkirk Bloodthief").expect("true printed registration");
    let record = meaning::meaning(&selector, id, 0).unwrap();
    meaning::validate(&record).unwrap();
    assert!(record.rule.noncreature_vampire_permanent_is_legal);
    assert!(!record.public_human_model_and_sampler_admitted);
    let mut wrong = record.clone();
    wrong.rule.noncreature_vampire_permanent_is_legal = false;
    assert!(meaning::validate(&wrong).is_err());
    let mut wrong = record.clone();
    wrong.selector.card_db_hash ^= 1;
    assert!(meaning::validate(&wrong).is_err());
    let mut wrong = record.clone();
    wrong.trigger_index = 1;
    assert!(meaning::validate(&wrong).is_err());
    let mut wrong = serde_json::to_value(&record).unwrap();
    wrong["undeclared"] = true.into();
    assert!(serde_json::from_value::<meaning::MeaningV1>(wrong).is_err());
}

#[test]
fn original_observer_controller_survives_real_control_response_and_coherent_rewrites_refuse() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        let target = put(&mut state, actor, "Vampire Soulcaller", Zone::Battlefield);
        event::propose_and_commit(&mut state, ProposedEvent::life_payment(actor.opponent(), 1));
        end_targets(&mut state).unwrap();
        let pending = restore(&state);
        announce(&mut state, source, target, actor);
        change_controller(&mut state, source, actor.opponent());
        let announced = restore(&state);
        for origin in [&pending, &announced] {
            for case in 0..4 {
                let mut bad = restore(origin);
                match case {
                    0 => {
                        for p in &mut bad.engine.pending_triggers {
                            if p.source == source {
                                p.controller = actor.opponent();
                                p.source_contract.as_mut().unwrap().controller = actor.opponent();
                            }
                        }
                        for i in &mut bad.stack {
                            if i.source == source {
                                i.controller = actor.opponent();
                                i.v4.ability_source_contract.as_mut().unwrap().controller =
                                    actor.opponent();
                            }
                        }
                    }
                    1 => bad.engine.event_history.retain(|e| {
                        !matches!(e, CommittedEvent::StromkirkEndStepObserversV1 { .. })
                    }),
                    2 => {
                        let row = bad
                            .engine
                            .event_history
                            .iter()
                            .find(|e| {
                                matches!(e, CommittedEvent::StromkirkEndStepObserversV1 { .. })
                            })
                            .unwrap()
                            .clone();
                        bad.engine.event_history.push(row);
                    }
                    _ => {
                        let row = bad
                            .engine
                            .event_history
                            .iter()
                            .find(|e| {
                                matches!(e, CommittedEvent::StromkirkEndStepObserversV1 { .. })
                            })
                            .unwrap()
                            .clone();
                        bad.engine.event_log.push(row);
                    }
                }
                let bytes = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err(), "case {case}");
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
                assert!(matches!(
                    engine::advance_until_decision(&mut bad),
                    Decision::Halted { .. }
                ));
                bad.engine.halted = None;
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            }
        }
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        let mut replay = restore(&state);
        for s in [&mut state, &mut replay] {
            settle(s);
            assert_eq!(s.objects.get(target).counters.plus1_plus1, 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn actual_trigger_accepts_typed_noncreature_vampire_permanent_and_places_counter() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        // Explicit physical subtype operand, not a claimed printed subtype-grant spell.
        let target = put(&mut state, actor, "Forest", Zone::Battlefield);
        state.objects.get_mut(target).v4.effective_subtype_ids = vec![Subtype::Vampire.stable_id()];
        assert!(!engine::object_has_type(&state, target, CardType::Creature));
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        assert!(end_targets(&mut state)
            .unwrap()
            .contains(&Target::Object(target)));
        announce(&mut state, source, target, actor);
        let mut replay = restore(&state);
        for value in [&mut state, &mut replay] {
            settle(value);
            assert_eq!(value.objects.get(target).counters.plus1_plus1, 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

fn refuses_corruption_readonly(state: &GameState) {
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
fn mandatory_capture_survives_erased_pending_and_published_rows_or_fabricated_completion() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        end_targets(&mut state).unwrap();
        let pending = restore(&state);
        announce(&mut state, source, source, actor);
        let published = restore(&state);
        for origin in [&pending, &published] {
            let capture = origin
                .engine
                .event_history
                .iter()
                .enumerate()
                .find_map(|(at, e)| {
                    matches!(e, CommittedEvent::StromkirkTriggerCaptureV2 { .. })
                        .then_some(at as u64)
                })
                .unwrap();
            for case in 0..4 {
                let mut bad = restore(origin);
                bad.engine.pending_triggers.clear();
                bad.stack.retain(|i| i.source != source);
                match case {
                    0 => {}
                    1 => {
                        let operation = bad.engine.event_history.len() as u64;
                        bad.engine.event_history.push(
                            serde_json::from_value(serde_json::json!({
                                "StromkirkOperationV2": {"capture_history_index":capture,
                                    "removal_history_index":capture, "outcome":"Countered"}
                            }))
                            .unwrap(),
                        );
                        bad.engine
                            .event_history
                            .push(CommittedEvent::StromkirkCompletedV2 {
                                capture_history_index: capture,
                                operation_history_index: operation,
                            });
                    }
                    2 => bad
                        .engine
                        .event_history
                        .retain(|e| !matches!(e, CommittedEvent::StromkirkCollectedV2 { .. })),
                    _ => bad
                        .engine
                        .event_history
                        .push(CommittedEvent::StromkirkCompletedV2 {
                            capture_history_index: capture,
                            operation_history_index: capture,
                        }),
                }
                refuses_corruption_readonly(&bad);
                assert_eq!(bad.objects.get(source).counters.plus1_plus1, 0);
            }
        }
        let mut bad = restore(&published);
        bad.stack.last_mut().unwrap().inline_effect = Some(EffectOp::Sequence(vec![]));
        refuses_corruption_readonly(&bad);
        let mut bad = restore(&published);
        bad.stack.push(bad.stack.last().unwrap().clone());
        refuses_corruption_readonly(&bad);
    }
}

#[test]
fn completed_real_counter_resolution_cannot_replay_saved_finalized_row_or_lose_operation() {
    use mtg_kernel::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        event::propose_and_commit(&mut state, ProposedEvent::life_payment(actor.opponent(), 1));
        end_targets(&mut state).unwrap();
        announce(&mut state, source, source, actor);
        let saved = state.stack.last().unwrap().clone();
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        for case in 0..4 {
            let mut bad = restore(&state);
            match case {
                0 => bad.stack.push(saved.clone()),
                1 => bad
                    .engine
                    .event_history
                    .retain(|e| !matches!(e, CommittedEvent::StromkirkResolutionRemovedV2 { .. })),
                2 => bad
                    .engine
                    .event_history
                    .retain(|e| !matches!(e, CommittedEvent::StromkirkOperationV2 { .. })),
                _ => {
                    let closed = bad
                        .engine
                        .event_history
                        .iter()
                        .find(|e| matches!(e, CommittedEvent::StromkirkCompletedV2 { .. }))
                        .unwrap()
                        .clone();
                    bad.engine.event_history.push(closed);
                }
            }
            refuses_corruption_readonly(&bad);
            assert_eq!(bad.objects.get(source).counters.plus1_plus1, 1);
        }
    }
}

#[test]
fn genuine_pending_source_departure_with_no_remaining_vampire_closes_without_publication() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        end_targets(&mut state).unwrap();
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
        assert!(state.engine.event_history.iter().any(|e| matches!(
            e,
            mtg_kernel::event::CommittedEvent::StromkirkCompletedV2 { .. }
        )));
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
