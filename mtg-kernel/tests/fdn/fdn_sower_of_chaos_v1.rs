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
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rules_meaning_sower_v1::{self as meaning, ErrorV1};
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
fn source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Sower of Chaos", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 4;
    priority(state, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    source
}
fn announce(state: &mut GameState, actor: PlayerId, source: ObjectId, target: ObjectId) {
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 3;
    priority(state, actor);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. }
        if activatable_abilities.contains(&(source, 0)))
    );
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, remaining: 1, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    // Target answers alone do not finalize an activation.
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    assert!(state.stack.iter().any(|item| item.source == source));
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

#[test]
fn future_printed_metadata_and_mandatory_unrestricted_creature_meaning() {
    let id = card_id_by_name("Sower of Chaos").expect("parent registration prerequisite");
    let def = &CARD_DEFS[usize::from(id)];
    assert!(def.is_executable());
    assert_eq!(
        (def.mana_value, def.power, def.toughness),
        (4, Some(4), Some(3))
    );
    assert_eq!(def.cost.generic, 3);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Devil]);
    assert_eq!(def.colors, &[ManaColor::R]);
    let selector = meaning::live_selector_v1().unwrap();
    let m = meaning::meaning_v1(&selector, id, 0).unwrap();
    assert_eq!(m.activation_cost, "{2}{R}");
    assert_eq!(m.target_power_filter, "none");
    assert_eq!((m.target_minimum, m.target_maximum), (1, 1));
    assert!(!m.requires_tap && !m.sorcery_speed_only && !m.remove_by_ability_suppression);
    assert_eq!(
        meaning::meaning_v1(&selector, id, 1),
        Err(ErrorV1::WrongRoot)
    );
}
#[test]
fn mana_quote_unaffordable_activation_and_illegal_targets_are_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (red, colorless) in [(0, 3), (1, 1), (1, 2)] {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.objects.get_mut(source).tapped = true;
            // A real affordable Blood Token action holds priority even when
            // Sower's activation is unaffordable. It supplies no mana.
            put(&mut state, actor, "Blood Token", Zone::Battlefield);
            state.players[actor.index()].mana_pool = [0; 6];
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = red;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = colorless;
            // The fixture's land may pay a missing red or generic pip, so tap it
            // before measuring the exact already-produced pool.
            for &land in &state.players[actor.index()].battlefield.clone() {
                if state.objects.get(land).card_def == card_id_by_name("Mountain").unwrap() {
                    state.objects.get_mut(land).tapped = true;
                }
            }
            priority(&mut state, actor);
            let legal = red == 1 && colorless == 2;
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. }
                if activatable_abilities.contains(&(source, 0)) == legal)
            );
            let before = serde_json::to_vec(&state).unwrap();
            let result = engine::step(&mut state, Action::ActivateAbility(source, 0));
            if !legal {
                assert!(result.is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                continue;
            }
            result.unwrap();
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { legal_targets, can_finish: false, .. }
                if legal_targets.contains(&Target::Object(target)))
            );
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::ChooseTarget(Target::Player(actor))).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            next(&mut state);
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert!(restricted(&state, target));
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert!(
                state.objects.get(source).tapped,
                "activation did not require or undo a tap"
            );
        }
    }
}
#[test]
fn current_power_controller_and_source_departure_do_not_add_unprinted_filters() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..4 {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            if mode == 0 {
                state.objects.get_mut(target).counters.plus1_plus1 = 20;
            }
            announce(&mut state, actor, source, target);
            state.objects.get_mut(target).counters.plus1_plus1 = 30;
            if mode == 1 {
                control_primitive(&mut state, target, actor);
            }
            if mode == 2 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
            }
            if mode == 3 {
                control_primitive(&mut state, source, actor.opponent());
            }
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert!(restricted(&state, target));
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            if mode == 1 {
                control_primitive(&mut state, target, actor.opponent());
                assert!(restricted(&state, target));
            }
        }
    }
}
#[test]
fn original_target_departure_reentry_hexproof_and_protection_fizzle_without_rebinding() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..4 {
            let mut state = ready(actor);
            let source = source(&mut state, actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            announce(&mut state, actor, source, target);
            match mode {
                0 | 1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Graveyard),
                    );
                    if mode == 1 {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(target, Zone::Battlefield),
                        );
                    }
                }
                2 => granted_target_keyword_primitive(&mut state, target, Keywords::HEXPROOF),
                _ => granted_target_keyword_primitive(
                    &mut state,
                    target,
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                ),
            }
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert!(!restricted(&state, target));
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn rule_restriction_survives_witness_and_real_blocker_menu_then_expires_at_cleanup() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        announce(&mut state, actor, source, target);
        finish(&mut state);
        witness(&mut state, actor, target);
        assert!(restricted(&state, target));
        let attacker = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
        state.step = Step::DeclareAttackers;
        engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        let mut reached = false;
        for _ in 0..24 {
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareBlockers { legal_blockers, .. } => {
                    assert!(legal_blockers
                        .iter()
                        .all(|(_, blockers)| !blockers.contains(&target)));
                    let before = serde_json::to_vec(&state).unwrap();
                    assert!(engine::step(
                        &mut state,
                        Action::DeclareBlockers(vec![(target, attacker)])
                    )
                    .is_err());
                    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap();
                    reached = true;
                    break;
                }
                other => panic!("unexpected combat {other:?}"),
            }
        }
        assert!(reached);
        // Follow the real combat through end step. Both hands retain a Bolt
        // so priority cannot auto-pass the observed boundary.
        for _ in 0..24 {
            let d = next(&mut state);
            if state.step == Step::Main2 {
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.step, Step::Main2);
        let mut replay = restore(&state);
        cleanup(&mut state, actor);
        cleanup(&mut replay, actor);
        assert!(!restricted(&state, target));
        assert!(state.engine.until_end_of_turn.is_empty());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn frozen_public_observations_explicitly_refuse_the_new_restriction() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        announce(&mut state, actor, source, target);
        finish(&mut state);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(mtg_kernel::rl::observe_policy_v8(
            &state,
            &PolicySurfaceV5::new(),
            actor,
            0,
            0,
            0,
            1
        )
        .is_err());
        assert!(mtg_kernel::policy_observation_v9::observe_policy_v9(
            &state,
            &PolicySurfaceV5::new(),
            actor,
            0,
            0,
            0,
            1
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[test]
fn activation_during_real_madness_cleanup_priority_expires_before_the_player_flip() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let bolt = state.players[actor.index()].hand[0];
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(bolt, Zone::Graveyard),
        );
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let temper = put(&mut state, actor, "Fiery Temper", Zone::Hand);
        for _ in 0..8 {
            event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        }
        assert_eq!(state.players[actor.index()].hand.len(), 9);
        let forest = *state.players[actor.index()].hand.last().unwrap();
        assert_eq!(state.objects.get(forest).name, "Forest");
        state.step = Step::End;
        let round = state.turn;
        let mut discarded = false;
        for _ in 0..20 {
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::Discard {
                    player,
                    count,
                    choices,
                } => {
                    assert_eq!((player, count), (actor, 2));
                    assert_eq!(state.step, Step::Cleanup);
                    assert!(choices.contains(&temper) && choices.contains(&forest));
                    engine::step(&mut state, Action::Discard(vec![temper, forest])).unwrap();
                    discarded = true;
                    break;
                }
                other => panic!("unexpected pre-cleanup decision {other:?}"),
            }
        }
        assert!(discarded);
        assert_eq!(state.objects.get(temper).zone, Zone::Exile);
        assert!(state.engine.pending_triggers.iter().any(|trigger| {
            trigger.is_madness_offer && trigger.source == temper && trigger.controller == actor
        }));
        // Discard queues the trigger. The real next advance publishes it
        // before either player receives the Cleanup response priority.
        priority(&mut state, actor);
        assert_eq!(
            (state.step, state.turn, state.active_player),
            (Step::Cleanup, round, actor)
        );
        assert!(state.stack.iter().any(|item| item.madness_offer));
        announce(&mut state, actor, source, target);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            let mut installed = false;
            for _ in 0..20 {
                let d = next(branch);
                if restricted(branch, target) {
                    assert_eq!(branch.step, Step::Cleanup);
                    assert_eq!((branch.turn, branch.active_player), (round, actor));
                    assert!(branch.stack.iter().any(|item| item.madness_offer));
                    installed = true;
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(branch, Action::Pass).unwrap();
            }
            assert!(installed);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        let mut resolved_replay = restore(&state);
        for branch in [&mut state, &mut replay, &mut resolved_replay] {
            let mut declined = false;
            for _ in 0..30 {
                let d = next(branch);
                if branch.active_player != actor {
                    break;
                }
                match d {
                    Decision::CastSpellOrPass { .. } => engine::step(branch, Action::Pass).unwrap(),
                    Decision::ChooseMadnessCast { player, card } => {
                        assert_eq!((player, card), (actor, temper));
                        assert_eq!(branch.step, Step::Cleanup);
                        engine::step(branch, Action::ChooseMadnessCast(false)).unwrap();
                        declined = true;
                    }
                    other => panic!("unexpected real cleanup decision {other:?}"),
                }
            }
            assert!(declined);
            assert_eq!(branch.active_player, actor.opponent());
            assert!(!restricted(branch, target));
            assert!(branch.engine.until_end_of_turn.is_empty());
            assert_eq!(branch.objects.get(target).controller, actor.opponent());
            assert_eq!(branch.objects.get(temper).zone, Zone::Graveyard);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&resolved_replay).unwrap()
        );
    }
}

#[test]
fn off_turn_activation_expires_at_that_turns_cleanup_for_both_controller_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = source(&mut state, actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        // Skip combat setup, then traverse the real end/cleanup/untap player flip.
        state.step = Step::Main2;
        cleanup(&mut state, actor);
        assert_eq!(state.active_player, actor.opponent());
        announce(&mut state, actor, source, target);
        finish(&mut state);
        assert!(restricted(&state, target));
        assert_eq!(state.active_player, actor.opponent());
        state.step = Step::Main2;
        let mut replay = restore(&state);
        cleanup(&mut state, actor.opponent());
        cleanup(&mut replay, actor.opponent());
        assert_eq!(state.active_player, actor);
        assert!(!restricted(&state, target));
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
