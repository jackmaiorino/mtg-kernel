//! Prepared v71 Reassembling Skeleton cases, pending serial admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CostComponent, Subtype, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

fn ready(player: PlayerId) -> GameState {
    let plains = card_id_by_name("Plains").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[plains; 40],
        &[plains; 40],
        |_| "Plains".into(),
        710,
        player,
    );
    state.step = Step::Main1;
    state.players[player.index()].mana_pool[ManaColor::B.pool_index()] = 2;
    state.players[player.index()].mana_pool[5] = 2;
    state
}

fn put(state: &mut GameState, player: PlayerId, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name("Reassembling Skeleton").unwrap();
    let source = state.objects.push(GameObject {
        card_def,
        name: "Reassembling Skeleton".into(),
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
        Zone::Battlefield => state.players[player.index()].battlefield.push(source),
        Zone::Hand => state.players[player.index()].hand.push(source),
        Zone::Graveyard => state.players[player.index()].graveyard.push(source),
        _ => panic!("fixture zone"),
    }
    source
}

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn offered(state: &mut GameState, source: ObjectId) -> bool {
    matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. }
        if activatable_abilities.contains(&(source, 0)))
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
    panic!("ability did not settle")
}

fn activate(state: &mut GameState, source: ObjectId) {
    assert!(offered(state, source));
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_activation.is_none());
    assert!(!state.stack.is_empty());
}

#[test]
fn skeleton_characteristics_and_graveyard_activation_recipe_are_exact() {
    let id = card_id_by_name("Reassembling Skeleton").unwrap();
    assert_eq!(id, 345);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.subtypes, &[Subtype::Skeleton, Subtype::Warrior]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(1), Some(1), 2)
    );
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::B)]);
    let ability = &def.activated_abilities[0];
    assert_eq!(ability.activation_zone, Zone::Graveyard);
    assert!(!ability.sorcery_speed_only);
    assert!(matches!(ability.cost, [CostComponent::Mana(cost)]
        if cost.generic == 1 && cost.pips == [Pip::Colored(ManaColor::B)]));
    assert!(matches!(
        (ability.effect)(),
        EffectOp::ReturnAbilitySourceFromGraveyard { tapped: true }
    ));
}

#[test]
fn skeleton_activation_requires_own_graveyard_and_full_mana() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        for zone in [Zone::Hand, Zone::Battlefield] {
            let wrong_zone = put(&mut state, player, zone);
            assert!(!offered(&mut state, wrong_zone));
        }
        let foreign = put(&mut state, player.opponent(), Zone::Graveyard);
        assert!(!offered(&mut state, foreign));
        let source = put(&mut state, player, Zone::Graveyard);
        state.players[player.index()].mana_pool = [0; 6];
        assert!(!offered(&mut state, source));
        state.players[player.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        assert!(!offered(&mut state, source));
        state.players[player.index()].mana_pool[5] = 1;
        activate(&mut state, source);
        settle(&mut state);
        let live = state.objects.get(source);
        assert_eq!(live.zone, Zone::Battlefield);
        assert!(live.tapped && live.summoning_sick);
        assert!(!live.v4.unearthed_v1);
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        assert_eq!(
            state.objects.get(source).zone,
            Zone::Graveyard,
            "no Unearth exile replacement"
        );
    }
}

#[test]
fn skeleton_pending_stack_restores_and_two_activations_return_only_once() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, Zone::Graveyard);
        activate(&mut state, source);
        activate(&mut state, source);
        assert_eq!(state.stack.len(), 2);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            settle(branch);
            let live = branch.objects.get(source);
            assert_eq!(live.zone, Zone::Battlefield);
            assert_eq!(live.zone_change_count, 1);
            assert!(live.tapped);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn skeleton_activation_does_not_follow_a_returned_graveyard_incarnation() {
    let mut state = ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, Zone::Graveyard);
    activate(&mut state, source);
    let mana_after_cost = state.players[0].mana_pool;
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    settle(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, mana_after_cost);
}

#[test]
fn skeleton_can_activate_at_opponents_end_step() {
    let mut state = ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, Zone::Graveyard);
    state.active_player = PlayerId::P1;
    state.step = Step::End;
    activate(&mut state, source);
    settle(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
}
