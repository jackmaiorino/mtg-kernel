//! Modal trigger placement, ferocious at resolution, and resumable choices.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardType, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, ObjectRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{SurfaceAction, SurfaceDecision};

fn ready() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state =
        GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
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
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        Zone::Hand => state.players[owner.index()].hand.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn pass(state: &mut GameState, decision: Decision) {
    match decision {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        other => panic!("unexpected priority: {other:?}"),
    }
}

fn finish(state: &mut GameState) {
    for _ in 0..60 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            return;
        }
        match decision {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            other => pass(state, other),
        }
    }
    panic!("resolution did not finish");
}

fn end(state: &mut GameState) -> Decision {
    state.step = Step::Main2;
    for _ in 0..30 {
        let decision = next(state);
        if state.step == Step::End {
            return decision;
        }
        pass(state, decision);
    }
    panic!("no end step");
}

fn scavenging(state: &mut GameState, player: PlayerId) -> ObjectId {
    put(state, player, "Sylvan Scavenging", Zone::Battlefield)
}

fn assert_mode(decision: Decision, player: PlayerId, source: ObjectId, legal: &[u8]) {
    assert!(matches!(decision,
        Decision::ChooseTriggerMode { player: actor, source: object, mode_count: 2, legal_modes }
            if actor == player && object == source && legal_modes == legal
    ));
}

fn choose_counter(state: &mut GameState, target: ObjectId) {
    engine::step(state, Action::ChooseTriggerMode(0)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { remaining: 1, legal_targets, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
}

fn choose_token(state: &mut GameState) {
    engine::step(state, Action::ChooseTriggerMode(1)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
}

fn tokens(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            state.objects.get(object).card_def == card_id_by_name("Raccoon Token").unwrap()
        })
        .collect()
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn printed_enchantment_token_and_deck_admission_are_exact() {
    let card = &CARD_DEFS[card_id_by_name("Sylvan Scavenging").unwrap() as usize];
    assert_eq!(card.mana_value, 3);
    assert_eq!(card.cost.generic, 1);
    assert_eq!(
        card.cost.pips,
        &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)]
    );
    assert_eq!(card.types, &[CardType::Enchantment]);
    assert_eq!(card.colors, &[ManaColor::G]);
    preflight_fully_supported_deck(&[card_id_by_name(card.name).unwrap()]).unwrap();
    let token = &CARD_DEFS[card_id_by_name("Raccoon Token").unwrap() as usize];
    assert_eq!((token.power, token.toughness), (Some(3), Some(3)));
    assert_eq!(token.colors, &[ManaColor::G]);
    assert_eq!(token.subtypes, &[Subtype::Raccoon]);
    assert!(token.is_token);
    assert_eq!(token.keywords, mtg_kernel::card_def::Keywords::NONE);
    assert!(token.activated_abilities.is_empty());
    assert!(preflight_fully_supported_deck(&[card_id_by_name(token.name).unwrap()]).is_err());
}

#[test]
fn cast_requires_three_mana_and_two_green_pips_without_mutation_on_rejection() {
    for (green, blue, legal) in [(1, 2, false), (2, 0, false), (0, 3, false), (2, 1, true)] {
        let mut state = ready();
        let spell = put(&mut state, PlayerId::P0, "Sylvan Scavenging", Zone::Hand);
        state.players[0].mana_pool[ManaColor::G.pool_index()] = green;
        state.players[0].mana_pool[ManaColor::U.pool_index()] = blue;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&spell) == legal)
        );
        let hash = state.state_hash();
        if legal {
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            finish(&mut state);
            assert_eq!(state.objects.get(spell).zone, Zone::Battlefield);
            assert_eq!(state.players[0].mana_pool, [0; 6]);
        } else {
            assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
            assert_eq!(state.state_hash(), hash);
        }
    }
}

#[test]
fn enchantment_cast_does_not_trigger_until_its_controllers_end_step() {
    let mut state = ready();
    let source = scavenging(&mut state, PlayerId::P1);
    assert!(matches!(end(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.is_empty());
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    assert_mode(end(&mut state), PlayerId::P1, source, &[1]);
}

#[test]
fn empty_board_still_offers_untargeted_token_mode_and_resolves_to_no_op() {
    let mut state = ready();
    let source = scavenging(&mut state, PlayerId::P0);
    assert_mode(end(&mut state), PlayerId::P0, source, &[1]);
    choose_token(&mut state);
    assert_eq!(state.stack.len(), 1);
    assert!(state.stack[0].targets.is_empty());
    finish(&mut state);
    assert!(tokens(&state, PlayerId::P0).is_empty());
}

#[test]
fn opponent_creatures_and_noncreatures_do_not_enable_counter_mode_or_ferocious() {
    let mut state = ready();
    let source = scavenging(&mut state, PlayerId::P0);
    put(
        &mut state,
        PlayerId::P1,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    assert_mode(end(&mut state), PlayerId::P0, source, &[1]);
    choose_token(&mut state);
    finish(&mut state);
    assert!(tokens(&state, PlayerId::P0).is_empty());
}

#[test]
fn small_creature_enables_both_modes_but_token_mode_does_nothing() {
    let mut state = ready();
    let source = scavenging(&mut state, PlayerId::P0);
    put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    assert_mode(end(&mut state), PlayerId::P0, source, &[0, 1]);
    choose_token(&mut state);
    finish(&mut state);
    assert!(tokens(&state, PlayerId::P0).is_empty());
}

#[test]
fn counter_mode_targets_only_controlled_creatures_before_any_priority() {
    let mut state = ready();
    let source = scavenging(&mut state, PlayerId::P0);
    let own = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    let opposing = put(&mut state, PlayerId::P1, "Elvish Mystic", Zone::Battlefield);
    let land = put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    assert_mode(end(&mut state), PlayerId::P0, source, &[0, 1]);
    assert!(state.stack.is_empty());
    engine::step(&mut state, Action::ChooseTriggerMode(0)).unwrap();
    assert!(
        matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets == [Target::Object(own)])
    );
    assert!(state.stack.is_empty());
    for action in [
        Action::Pass,
        Action::ChooseTarget(Target::Object(opposing)),
        Action::ChooseTarget(Target::Object(land)),
    ] {
        let hash = state.state_hash();
        assert!(engine::step(&mut state, action).is_err());
        assert_eq!(state.state_hash(), hash);
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(own))).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 1);
    finish(&mut state);
    assert_eq!(state.objects.get(own).counters.plus1_plus1, 1);
    assert_eq!(engine::effective_power(&state, own), 2);
}

#[test]
fn four_power_is_inclusive_and_creates_the_exact_token() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    end(&mut state);
    choose_token(&mut state);
    finish(&mut state);
    let token = tokens(&state, PlayerId::P0)[0];
    assert_eq!(
        (
            engine::effective_power(&state, token),
            engine::effective_toughness(&state, token)
        ),
        (3, 3)
    );
    assert_eq!(state.objects.get(token).owner, PlayerId::P0);
    assert!(state.objects.get(token).summoning_sick);
}

#[test]
fn three_power_does_not_create_a_token() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Beast-Kin Ranger",
        Zone::Battlefield,
    );
    end(&mut state);
    choose_token(&mut state);
    finish(&mut state);
    assert!(tokens(&state, PlayerId::P0).is_empty());
}

#[test]
fn ferocious_observes_counters_added_after_mode_selection() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Beast-Kin Ranger",
        Zone::Battlefield,
    );
    end(&mut state);
    choose_token(&mut state);
    let effect = EffectOp::AddPlusOnePlusOneCounters {
        object: ObjectRef::ThisSource,
        count: 1,
    };
    mtg_kernel::effect::execute(
        &effect,
        &mtg_kernel::effect::ExecCtx::no_targets(creature, PlayerId::P0),
        &mut state,
    );
    finish(&mut state);
    assert_eq!(tokens(&state, PlayerId::P0).len(), 1);
}

#[test]
fn ferocious_observes_creature_removed_in_response() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    end(&mut state);
    choose_token(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(creature, Zone::Graveyard),
    );
    finish(&mut state);
    assert!(tokens(&state, PlayerId::P0).is_empty());
}

#[test]
fn temporary_power_reduction_in_response_prevents_the_token() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    let creature = put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Fleeting Distraction", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    end(&mut state);
    choose_token(&mut state);
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
    finish(&mut state);
    assert_eq!(engine::effective_power(&state, creature), 3);
    assert!(tokens(&state, PlayerId::P0).is_empty());
}

#[test]
fn created_token_emits_entry_events_for_unicorn_and_angel() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Cackling Prowler",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Good-Fortune Unicorn",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Dazzling Angel",
        Zone::Battlefield,
    );
    end(&mut state);
    choose_token(&mut state);
    finish(&mut state);
    let token = tokens(&state, PlayerId::P0)[0];
    assert_eq!(state.players[0].life, 21);
    assert_eq!(state.objects.get(token).counters.plus1_plus1, 1);
}

#[test]
fn counter_fizzles_after_target_leaves_and_reenters() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    let creature = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    end(&mut state);
    choose_counter(&mut state, creature);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(creature, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(creature, Zone::Battlefield),
    );
    finish(&mut state);
    assert_eq!(state.objects.get(creature).counters.plus1_plus1, 0);
}

#[test]
fn both_modes_survive_the_enchantment_leaving_after_triggering() {
    for mode in [0, 1] {
        let mut state = ready();
        let source = scavenging(&mut state, PlayerId::P0);
        let creature = put(
            &mut state,
            PlayerId::P0,
            "Cackling Prowler",
            Zone::Battlefield,
        );
        end(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        assert_mode(next(&mut state), PlayerId::P0, source, &[0, 1]);
        if mode == 0 {
            choose_counter(&mut state, creature);
        } else {
            choose_token(&mut state);
        }
        finish(&mut state);
        assert_eq!(
            state.objects.get(creature).counters.plus1_plus1,
            i32::from(mode == 0)
        );
        assert_eq!(tokens(&state, PlayerId::P0).len(), usize::from(mode == 1));
    }
}

#[test]
fn simultaneous_triggers_order_before_modes_and_targets() {
    let mut state = ready();
    let first = scavenging(&mut state, PlayerId::P0);
    let second = scavenging(&mut state, PlayerId::P0);
    let creature = put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    assert!(
        matches!(end(&mut state), Decision::OrderTriggers { ref pending, .. } if pending.len() == 2)
    );
    let hash = state.state_hash();
    assert!(engine::step(&mut state, Action::ChooseTriggerMode(0)).is_err());
    assert_eq!(state.state_hash(), hash);
    engine::step(&mut state, Action::OrderTriggers(vec![1, 0])).unwrap();
    assert_mode(next(&mut state), PlayerId::P0, second, &[0, 1]);
    engine::step(&mut state, Action::ChooseTriggerMode(0)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
    assert_mode(next(&mut state), PlayerId::P0, first, &[0, 1]);
    assert_eq!(state.stack.len(), 1);
    choose_token(&mut state);
    assert_eq!(state.stack.len(), 2);
    finish(&mut state);
    assert_eq!(state.objects.get(creature).counters.plus1_plus1, 1);
}

#[test]
fn pending_mode_rejects_wrong_actions_and_unavailable_indices_without_mutation() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    end(&mut state);
    for action in [
        Action::Pass,
        Action::ChooseTriggerMode(0),
        Action::ChooseTriggerMode(2),
        Action::ChooseSpellMode(1),
        Action::ChooseEffectOption(1),
    ] {
        let hash = state.state_hash();
        assert!(engine::step(&mut state, action).is_err());
        assert_eq!(state.state_hash(), hash);
    }
}

#[test]
fn policy_candidates_preserve_sparse_printed_indices_and_trigger_actions() {
    let mut state = ready();
    let source = scavenging(&mut state, PlayerId::P0);
    let decision = end(&mut state);
    let candidates =
        legal_action_candidates_v1(&SurfaceDecision::Decision(decision), &state).unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(matches!(&candidates[0].record.semantic,
        ActionSemanticV1::ChooseSpellMode { source: card, mode_index: 1, mode_count: 2, .. }
            if card.arena_id == source.0));
    assert!(matches!(
        candidates[0].surface_action,
        SurfaceAction::Action(Action::ChooseTriggerMode(1))
    ));
}

#[test]
fn restore_preserves_unselected_mode_and_both_selected_branches() {
    for stage in 0..4 {
        let mut state = ready();
        scavenging(&mut state, PlayerId::P0);
        let creature = put(
            &mut state,
            PlayerId::P0,
            "Cackling Prowler",
            Zone::Battlefield,
        );
        end(&mut state);
        if stage > 0 {
            engine::step(&mut state, Action::ChooseTriggerMode(u8::from(stage == 3))).unwrap();
        }
        if stage == 2 {
            next(&mut state);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
        }
        let mut copy = restored(&state);
        assert_eq!(state.state_hash(), copy.state_hash());
        assert_eq!(next(&mut state), next(&mut copy));
        for game in [&mut state, &mut copy] {
            if stage == 0 {
                engine::step(game, Action::ChooseTriggerMode(0)).unwrap();
                next(game);
            }
            if stage < 2 {
                engine::step(game, Action::ChooseTarget(Target::Object(creature))).unwrap();
            }
            finish(game);
        }
        assert_eq!(state.state_hash(), copy.state_hash());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&copy).unwrap()
        );
    }
}

#[test]
fn selected_mode_target_spec_is_authenticated_on_restore() {
    let mut state = ready();
    scavenging(&mut state, PlayerId::P0);
    put(&mut state, PlayerId::P0, "Elvish Mystic", Zone::Battlefield);
    end(&mut state);
    engine::step(&mut state, Action::ChooseTriggerMode(0)).unwrap();
    state.engine.pending_triggers[0].target_spec = TargetSpec::None;
    let mut copy = restored(&state);
    assert!(matches!(
        engine::advance_until_decision(&mut copy),
        Decision::Halted { .. }
    ));
}
