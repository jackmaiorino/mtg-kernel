//! Unregistered printed acceptance fixtures. Casting requires the variable-target successor.
//! These fixtures are unexecuted and deliberately do not inject a definition or bypass roots.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name)
        .unwrap_or_else(|| panic!("printed fixture requires registration: {name}"));
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
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("unsupported setup zone"),
    }
    id
}

fn ready(actor: PlayerId) -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[island; 40],
        &[island; 40],
        |_| "Island".into(),
        11004,
        actor,
    );
    state.step = Step::Main1;
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    state
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision @ Decision::Halted { .. }) => panic!("{decision:?}"),
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("unexpected surface result {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn restored(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let replay: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&replay).unwrap());
    assert_eq!(state.state_hash(), replay.state_hash());
    replay
}

fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    for _ in 0..100 {
        let decision = next(surface, state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => panic!("unexpected resolution decision {other:?}"),
        };
        apply(surface, state, action);
    }
    panic!("resolution did not finish");
}

fn drain_restored(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    let mut replay = restored(state);
    drain(surface, state);
    drain(&mut self::surface(), &mut replay);
    assert_eq!(
        serde_json::to_vec(state).unwrap(),
        serde_json::to_vec(&replay).unwrap()
    );
    assert_eq!(state.state_hash(), replay.state_hash());
}

fn announce(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
) -> ObjectId {
    let spell = put(state, actor, name, Zone::Hand);
    // Exact payable choices: base W and kicker 2W remain real alternatives.
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] =
        if name == "Lightning Bolt" { 1 } else { 0 };
    if matches!(next(surface, state), Decision::CastSpellOrPass { player, .. } if player != actor) {
        apply(surface, state, Action::Pass);
    }
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&spell))
    );
    apply(surface, state, Action::CastSpell(spell));
    spell
}

fn choose_kicker(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    actor: PlayerId,
    kicked: bool,
) {
    assert!(
        matches!(next(surface, state), Decision::ChooseVariableCastKicker { player, can_decline, .. } if player == actor && (kicked || can_decline))
    );
    apply(surface, state, Action::ChooseKicker(kicked));
}

fn target(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    actor: PlayerId,
    object: ObjectId,
    can_finish: bool,
) {
    assert!(
        matches!(next(surface, state), Decision::ChooseTargets { player, legal_targets, can_finish: offered_finish, .. }
            | Decision::ChooseVariableCastTargets { player, legal_targets, can_finish: offered_finish, .. }
        if player == actor && offered_finish == can_finish && legal_targets.contains(&Target::Object(object)))
    );
    apply(surface, state, Action::ChooseTarget(Target::Object(object)));
}

fn finish_kicked(surface: &mut HarnessSurfaceV2, state: &mut GameState, actor: PlayerId) {
    assert!(
        matches!(next(surface, state), Decision::ChooseVariableCastTargets { player, can_finish: true, .. } if player == actor)
    );
    apply(surface, state, Action::FinishEffectSelection);
    assert!(matches!(
        next(surface, state),
        Decision::CastSpellOrPass { .. }
    ));
}

fn protected(state: &GameState, object: ObjectId) -> bool {
    engine::has_effective_keyword(state, object, Keywords::INDESTRUCTIBLE)
}

#[test]
fn divine_printed_definition_is_white_instant_with_2w_kicker() {
    let def = &CARD_DEFS
        [card_id_by_name("Divine Resilience").expect("unregistered printed acceptance") as usize];
    assert_eq!(def.capability, CardCapability::Full); // Future admission must prove the complete kicked rule.
    assert_eq!(def.types, &[CardType::Instant]);
    assert_eq!(def.colors, &[ManaColor::W]);
    assert_eq!(def.mana_value, 1);
    assert_eq!(def.cost.generic, 0);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::W)]);
    let kicker = def.kicker_cost.unwrap();
    assert_eq!(kicker.generic, 2);
    assert_eq!(kicker.pips, &[Pip::Colored(ManaColor::W)]);
}

#[test]
fn divine_unkicked_requires_one_controlled_creature_and_preserves_resolution_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let opponent = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mut surface = surface();
        let spell = announce(&mut surface, &mut state, actor, "Divine Resilience");
        choose_kicker(&mut surface, &mut state, actor, false);
        assert!(
            matches!(next(&mut surface, &mut state), Decision::ChooseTargets { can_finish: false, ref legal_targets, .. }
            if legal_targets.contains(&Target::Object(own)) && !legal_targets.contains(&Target::Object(opponent)))
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(opponent))).is_err());
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            target(&mut self::surface(), current, actor, own, false);
            drain(&mut self::surface(), current);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert!(protected(&state, own));
        assert!(!protected(&state, opponent));
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
    }
}

#[test]
fn divine_kicked_zero_one_and_three_distinct_creatures_are_actual_announced_targets() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (available, count) in [(0, 0), (4, 0), (4, 1), (4, 3)] {
            let mut state = ready(actor);
            let creatures: Vec<_> = (0..available)
                .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
                .collect();
            let opponent = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let mut surface = surface();
            let spell = announce(&mut surface, &mut state, actor, "Divine Resilience");
            choose_kicker(&mut surface, &mut state, actor, true);
            for &object in &creatures[..count] {
                let mut replay = restored(&state);
                target(&mut surface, &mut state, actor, object, true);
                target(&mut self::surface(), &mut replay, actor, object, true);
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
                assert!(
                    matches!(next(&mut surface, &mut state), Decision::ChooseVariableCastTargets { ref legal_targets, can_finish: true, .. }
                    if !legal_targets.contains(&Target::Object(object)) && !legal_targets.contains(&Target::Object(opponent)))
                );
                let before = serde_json::to_vec(&state).unwrap();
                assert!(
                    engine::step(&mut state, Action::ChooseTarget(Target::Object(object))).is_err()
                );
                assert_eq!(before, serde_json::to_vec(&state).unwrap());
            }
            finish_kicked(&mut surface, &mut state, actor);
            let item = state
                .stack
                .iter()
                .find(|item| item.source == spell)
                .unwrap();
            assert!(item.kicked);
            assert_eq!(
                item.targets,
                creatures[..count]
                    .iter()
                    .copied()
                    .map(Target::Object)
                    .collect::<Vec<_>>()
            );
            assert_eq!(item.v4.target_contracts.len(), count);
            drain_restored(&mut surface, &mut state);
            for (index, &creature) in creatures.iter().enumerate() {
                assert_eq!(protected(&state, creature), index < count);
            }
            assert!(!protected(&state, opponent));
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::W.pool_index()],
                0
            );
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
        }
    }
}

#[test]
fn divine_kicked_rechecks_each_selected_incarnation_and_protection_without_changing_other_targets()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let mut state = ready(actor);
            let creatures: Vec<_> = (0..3)
                .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
                .collect();
            let mut surface = surface();
            announce(&mut surface, &mut state, actor, "Divine Resilience");
            choose_kicker(&mut surface, &mut state, actor, true);
            for &object in &creatures {
                target(&mut surface, &mut state, actor, object, true);
            }
            finish_kicked(&mut surface, &mut state, actor);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creatures[0], Zone::Exile),
            );
            if reenter {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(creatures[0], Zone::Battlefield),
                );
            }
            // Existing temporary effect primitive creates protection after announcement.
            let object = creatures[1];
            let generation = state.objects.get(object).zone_change_count;
            state.engine.until_end_of_turn.push(
                engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                    object_id: object,
                    object_zone_change_count: generation,
                    layer: engine::Layers::ABILITY_ADDING,
                    timestamp: 1,
                    duration: engine::EffectDuration::EndOfTurn,
                    keywords: Keywords::PROTECTION_FROM_MONOCOLORED,
                },
            );
            drain_restored(&mut surface, &mut state);
            assert!(!protected(&state, creatures[0]));
            assert!(!protected(&state, creatures[1]));
            assert!(protected(&state, creatures[2]));
        }
    }
}

#[test]
fn divine_unkicked_departure_reentry_or_control_change_invalidates_the_only_target() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for change in 0..3 {
            let mut state = ready(actor);
            let object = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let mut surface = surface();
            let spell = announce(&mut surface, &mut state, actor, "Divine Resilience");
            choose_kicker(&mut surface, &mut state, actor, false);
            target(&mut surface, &mut state, actor, object, false);
            next(&mut surface, &mut state); // Finish payment before the response setup.
            if change < 2 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(object, Zone::Exile),
                );
                if change == 1 {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(object, Zone::Battlefield),
                    );
                }
            } else {
                state.players[actor.index()]
                    .battlefield
                    .retain(|&id| id != object);
                state.players[actor.opponent().index()]
                    .battlefield
                    .push(object);
                state.objects.get_mut(object).controller = actor.opponent();
            }
            drain_restored(&mut surface, &mut state);
            assert!(!protected(&state, object));
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        }
    }
}

#[test]
fn divine_indestructible_keeps_lethal_damage_until_real_cleanup_then_expires() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let mut surface = surface();
        announce(&mut surface, &mut state, actor, "Divine Resilience");
        choose_kicker(&mut surface, &mut state, actor, false);
        target(&mut surface, &mut state, actor, creature, false);
        drain_restored(&mut surface, &mut state);
        let bolt = announce(&mut surface, &mut state, actor, "Lightning Bolt");
        target(&mut surface, &mut state, actor, creature, false);
        drain_restored(&mut surface, &mut state);
        assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(creature).damage, 3); // Indestructible does not prevent damage.
        assert!(protected(&state, creature));
        state.step = Step::EndStep; // Prepare a normal end-step priority window, then take real passes.
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            let mut surface = self::surface();
            for _ in 0..16 {
                if !protected(current, creature) {
                    break;
                }
                assert!(matches!(
                    next(&mut surface, current),
                    Decision::CastSpellOrPass { .. }
                ));
                apply(&mut surface, current, Action::Pass);
            }
            next(&mut surface, current);
            assert!(!protected(current, creature));
            assert_eq!(current.objects.get(creature).damage, 0);
            assert_eq!(current.objects.get(creature).zone, Zone::Battlefield);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn divine_announced_target_prefix_cannot_restore_as_unkicked_after_three_choices() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let creatures: Vec<_> = (0..4)
            .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
            .collect();
        let mut surface = surface();
        announce(&mut surface, &mut state, actor, "Divine Resilience");
        choose_kicker(&mut surface, &mut state, actor, true);
        for &object in &creatures[..3] {
            target(&mut surface, &mut state, actor, object, true);
        }
        let mut forged = restored(&state);
        forged.engine.pending_cast.as_mut().unwrap().kicked = Some(false);
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(engine::step(&mut forged, Action::FinishEffectSelection).is_err());
        assert_eq!(before, serde_json::to_vec(&forged).unwrap());
        assert!(creatures.iter().all(|&object| !protected(&forged, object)));
        finish_kicked(&mut surface, &mut state, actor);
        drain_restored(&mut surface, &mut state);
        assert!(creatures[..3]
            .iter()
            .all(|&object| protected(&state, object)));
        assert!(!protected(&state, creatures[3]));
    }
}

#[test]
fn divine_any_number_uses_more_than_255_exact_targets_and_restores_finish() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let creatures: Vec<_> = (0..300)
            .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
            .collect();
        let mut surface = surface();
        let spell = announce(&mut surface, &mut state, actor, "Divine Resilience");
        choose_kicker(&mut surface, &mut state, actor, true);
        for &object in &creatures {
            target(&mut surface, &mut state, actor, object, true);
        }
        assert!(
            matches!(next(&mut surface, &mut state), Decision::ChooseVariableCastTargets { selected_count: 300, can_finish: true, legal_targets, .. } if legal_targets.is_empty())
        );
        let mut replay = restored(&state);
        finish_kicked(&mut surface, &mut state, actor);
        finish_kicked(&mut self::surface(), &mut replay, actor);
        assert_eq!(
            state
                .stack
                .iter()
                .find(|item| item.source == spell)
                .unwrap()
                .targets
                .len(),
            300
        );
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        drain_restored(&mut surface, &mut state);
        assert!(creatures.iter().all(|&object| protected(&state, object)));
    }
}

#[test]
fn divine_pending_and_finalized_kicker_bindings_reject_unilateral_restore_rewrites() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let mut surface = surface();
        let spell = announce(&mut surface, &mut state, actor, "Divine Resilience");
        choose_kicker(&mut surface, &mut state, actor, true);
        target(&mut surface, &mut state, actor, creature, true);
        let mut forged = restored(&state);
        forged.engine.pending_cast.as_mut().unwrap().kicked = Some(false);
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(engine::step(&mut forged, Action::ChooseTarget(Target::Object(creature))).is_err());
        assert_eq!(before, serde_json::to_vec(&forged).unwrap());
        finish_kicked(&mut surface, &mut state, actor);
        for erase in [false, true] {
            let mut forged = restored(&state);
            let item = forged
                .stack
                .iter_mut()
                .find(|item| item.source == spell)
                .unwrap();
            if erase {
                forged.objects.get_mut(spell).v4.finalized_cast_binding = None;
                forged.objects.get_mut(spell).v4.variable_targeting_kicked = None;
                item.v4
                    .source_contract
                    .as_mut()
                    .unwrap()
                    .finalized_cast_binding = None;
            } else {
                item.kicked = false;
            }
            let mut halted = false;
            for _ in 0..8 {
                match engine::advance_until_decision(&mut forged) {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut forged, Action::Pass).unwrap();
                    }
                    Decision::Halted { .. } => {
                        halted = true;
                        break;
                    }
                    other => panic!("unexpected forged-cast decision {other:?}"),
                }
            }
            assert!(
                halted,
                "forged cast must halt when ordinary priority reaches resolution"
            );
            assert!(!protected(&forged, creature));
        }
    }
}

#[test]
fn divine_no_creature_kicker_cannot_decline_and_legacy_consumers_refuse() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let mut surface = surface();
        let spell = announce(&mut surface, &mut state, actor, "Divine Resilience");
        let decision = next(&mut surface, &mut state);
        assert!(
            matches!(decision, Decision::ChooseVariableCastKicker { player, can_decline: false, .. } if player == actor)
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ChooseKicker(false)).is_err());
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
        assert!(mtg_kernel::rl::observe_v1(&state, actor, 0)
            .unwrap_err()
            .to_string()
            .contains("successor"));
        let policy = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new_with_engine_priority_v1();
        for viewer in [actor, actor.opponent()] {
            assert!(mtg_kernel::rl::observe_v2(&state, &surface, viewer, 0).is_err());
            assert!(
                mtg_kernel::rl::observe_policy_v5(&state, &policy, viewer, 0, 0, 0, 1).is_err()
            );
            assert!(
                mtg_kernel::rl::observe_policy_v6(&state, &policy, viewer, 0, 0, 0, 1).is_err()
            );
            assert!(
                mtg_kernel::rl::observe_policy_v8(&state, &policy, viewer, 0, 0, 0, 1).is_err()
            );
            let error = mtg_kernel::rl::observe_v1(&state, viewer, 0)
                .unwrap_err()
                .to_string();
            assert!(error.contains("successor"));
            assert!(!error.contains("Divine") && !error.contains("Island"));
        }
        let offered = SurfaceDecision::Decision(decision);
        assert!(mtg_kernel::rl::legal_action_candidates_v1(&offered, &state).is_err());
        apply(&mut surface, &mut state, Action::ChooseKicker(true));
        finish_kicked(&mut surface, &mut state, actor);
        assert!(mtg_kernel::rl::observe_v1(&state, actor, 0).is_err());
        drain_restored(&mut surface, &mut state);
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
    }
}

#[test]
fn divine_empty_creature_pool_requires_payable_kicked_total_before_cast_offer() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let spell = put(&mut state, actor, "Divine Resilience", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. } if player == actor && !castable_spells.contains(&spell))
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. } if player == actor && castable_spells.contains(&spell))
        );
    }
}
