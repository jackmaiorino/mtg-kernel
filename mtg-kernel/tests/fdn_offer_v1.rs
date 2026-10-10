//! Prepared v74 An Offer You Can't Refuse cases, pending serial admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        740,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
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
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("spell did not resolve")
}

fn cast_think_twice(state: &mut GameState, controller: PlayerId) -> ObjectId {
    let source = put(state, controller, "Think Twice", Zone::Hand);
    state.players[controller.index()].mana_pool[5] = 1;
    state.players[controller.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, .. } if player == controller)
    );
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.stack.len(), 1);
    source
}

fn begin_offer(state: &mut GameState, caster: PlayerId, target_controller: PlayerId) -> ObjectId {
    let source = put(state, caster, "An Offer You Can't Refuse", Zone::Hand);
    if caster != target_controller {
        engine::step(state, Action::Pass).unwrap();
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player, .. } if player == caster)
        );
    }
    state.players[caster.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { player, .. } if player == caster));
    source
}

fn treasures(state: &GameState, controller: PlayerId) -> usize {
    let token = card_id_by_name("Treasure Token").unwrap();
    state
        .objects
        .iter()
        .filter(|(_, object)| {
            object.zone == Zone::Battlefield
                && object.controller == controller
                && object.card_def == token
        })
        .count()
}

#[test]
fn offer_characteristics_and_noncreature_counter_reward_program_are_exact() {
    let id = card_id_by_name("An Offer You Can't Refuse").unwrap();
    assert_eq!(id, 348);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Instant]);
    assert_eq!(def.mana_value, 1);
    assert_eq!(def.cost.generic, 0);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert_eq!(def.target_spec, TargetSpec::NoncreatureSpellOnStack);
    assert!(
        matches!((def.spell_effect)(), Some(EffectOp::CounterTargetSpellThenCreateTokens {
        target_index: 0, token_def, count: 2,
    }) if token_def == card_id_by_name("Treasure Token").unwrap())
    );
}

#[test]
fn offer_counters_own_or_opposing_spell_rewards_its_controller_and_restores_target_choice() {
    for target_controller in [PlayerId::P0, PlayerId::P1] {
        for caster in [target_controller, target_controller.opponent()] {
            let mut state = ready(target_controller);
            let target = cast_think_twice(&mut state, target_controller);
            let library_before = state.players[target_controller.index()].library.len();
            let offer = begin_offer(&mut state, caster, target_controller);
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut replay] {
                engine::step(branch, Action::ChooseTarget(Target::Object(target))).unwrap();
                assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
                assert!(branch.engine.pending_cast.is_none());
                assert_eq!(branch.stack.len(), 2);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
            let mut stack_restore: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut replay, &mut stack_restore] {
                settle(branch);
                assert_eq!(branch.objects.get(target).zone, Zone::Graveyard);
                assert_eq!(branch.objects.get(offer).zone, Zone::Graveyard);
                assert_eq!(
                    branch.players[target_controller.index()].library.len(),
                    library_before
                );
                assert_eq!(treasures(branch, target_controller), 2);
                assert_eq!(treasures(branch, target_controller.opponent()), 0);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
            assert_eq!(state.state_hash(), stack_restore.state_hash());
        }
    }
}

#[test]
fn offer_with_departed_target_fizzles_without_reward() {
    for target_controller in [PlayerId::P0, PlayerId::P1] {
        let caster = target_controller.opponent();
        let mut state = ready(target_controller);
        let target = cast_think_twice(&mut state, target_controller);
        let offer = begin_offer(&mut state, caster, target_controller);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            settle(branch);
            assert_eq!(branch.objects.get(offer).zone, Zone::Graveyard);
            assert_eq!(treasures(branch, PlayerId::P0), 0);
            assert_eq!(treasures(branch, PlayerId::P1), 0);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn offer_is_not_castable_when_only_a_creature_spell_is_on_stack() {
    for target_controller in [PlayerId::P0, PlayerId::P1] {
        let caster = target_controller.opponent();
        let mut state = ready(target_controller);
        let target = put(
            &mut state,
            target_controller,
            "Faerie Miscreant",
            Zone::Hand,
        );
        let offer = put(&mut state, caster, "An Offer You Can't Refuse", Zone::Hand);
        state.players[target_controller.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(target)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        engine::step(&mut state, Action::Pass).unwrap();
        state.players[caster.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == caster)
        );
        let before = state.state_hash();
        assert!(engine::step(&mut state, Action::CastSpell(offer)).is_err());
        assert_eq!(state.state_hash(), before);
    }
}
