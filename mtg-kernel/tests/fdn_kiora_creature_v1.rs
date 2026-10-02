//! Kiora's ordered loot, intervening threshold and optional legendary token.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

fn ready() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state =
        GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 123);
    state.step = Step::Main1;
    enable_foundations_combat_v1(&mut state).unwrap();
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
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
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        Zone::Hand => state.players[player.index()].hand.push(object),
        Zone::Graveyard => state.players[player.index()].graveyard.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn pass_or_order(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected decision {other:?}"),
    };
    engine::step(state, action).unwrap();
}

fn drain(state: &mut GameState, option: Option<u16>) {
    for _ in 0..100 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            return;
        }
        match decision {
            Decision::Discard { choices, count, .. } => {
                engine::step(
                    state,
                    Action::Discard(choices.into_iter().take(count as usize).collect()),
                )
                .unwrap();
            }
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 2);
                engine::step(
                    state,
                    Action::ChooseEffectOption(option.expect("explicit optional answer")),
                )
                .unwrap();
            }
            other => pass_or_order(state, other),
        }
    }
    panic!("resolution did not finish");
}

fn reach_option(state: &mut GameState) {
    for _ in 0..100 {
        let decision = next(state);
        if let Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        } = decision
        {
            assert_eq!((player, option_count), (PlayerId::P0, 2));
            return;
        }
        assert!(
            !state.stack.is_empty() || !state.engine.pending_triggers.is_empty(),
            "missing threshold trigger"
        );
        pass_or_order(state, decision);
    }
    panic!("optional choice did not appear");
}

fn attack(graveyard_cards: usize) -> (GameState, ObjectId) {
    let mut state = ready();
    let kiora = put(
        &mut state,
        PlayerId::P0,
        "Kiora, the Rising Tide",
        Zone::Battlefield,
    );
    for _ in 0..graveyard_cards {
        put(&mut state, PlayerId::P0, "Island", Zone::Graveyard);
    }
    state.step = Step::DeclareAttackers;
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    engine::step(&mut state, Action::DeclareAttackers(vec![kiora])).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    (state, kiora)
}

fn scions(state: &GameState) -> Vec<ObjectId> {
    state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|id| {
            state.objects.get(*id).card_def == card_id_by_name("Scion of the Deep Token").unwrap()
        })
        .collect()
}

#[test]
fn printed_kiora_and_scion_characteristics_and_admission_are_exact() {
    assert_eq!(card_id_by_name("Kiora, the Rising Tide"), Some(197));
    assert_eq!(card_id_by_name("Scion of the Deep Token"), Some(198));
    let kiora = &CARD_DEFS[197];
    assert_eq!(
        (kiora.power, kiora.toughness, kiora.mana_value),
        (Some(3), Some(2), 3)
    );
    assert_eq!(kiora.cost.generic, 2);
    assert_eq!(kiora.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert_eq!(kiora.colors, &[ManaColor::U]);
    assert_eq!(kiora.subtypes, &[Subtype::Merfolk, Subtype::Noble]);
    assert_eq!(kiora.supertypes, &[Supertype::Legendary]);
    let token = &CARD_DEFS[198];
    assert_eq!(token.object_name, "Scion of the Deep");
    assert_eq!(
        (token.power, token.toughness, token.mana_value),
        (Some(8), Some(8), 0)
    );
    assert_eq!(token.colors, &[ManaColor::U]);
    assert_eq!(token.subtypes, &[Subtype::Octopus]);
    assert_eq!(token.supertypes, &[Supertype::Legendary]);
    assert!(token.is_token);
    preflight_fully_supported_deck(&[197]).unwrap();
    assert!(preflight_fully_supported_deck(&[198]).is_err());
}

#[test]
fn entry_draws_two_before_controller_discards_two_and_restores_that_choice() {
    let mut state = ready();
    let forest = put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    let kiora = put(
        &mut state,
        PlayerId::P0,
        "Kiora, the Rising Tide",
        Zone::Hand,
    );
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 3;
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&kiora))
    );
    engine::step(&mut state, Action::CastSpell(kiora)).unwrap();
    let choices = (0..100)
        .find_map(|_| {
            let decision = next(&mut state);
            if let Decision::Discard {
                player,
                count,
                choices,
            } = decision
            {
                assert_eq!((player, count), (PlayerId::P0, 2));
                return Some(choices);
            }
            pass_or_order(&mut state, decision);
            None
        })
        .expect("entry discard decision");
    assert_eq!(state.objects.get(kiora).zone, Zone::Battlefield);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(state.players[0].library.len(), 38);
    assert_eq!(state.players[0].hand.len(), 3);
    assert!(choices.contains(&forest));
    let drawn = *choices.iter().find(|id| **id != forest).unwrap();
    let saved = serde_json::to_vec(&state).unwrap();
    engine::step(&mut state, Action::Discard(vec![forest, drawn])).unwrap();
    drain(&mut state, None);
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    engine::step(&mut restored, Action::Discard(vec![forest, drawn])).unwrap();
    drain(&mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].graveyard.len(), 2);
}

#[test]
fn attack_with_six_cards_does_not_trigger_even_with_opponent_threshold() {
    let (mut state, _) = attack(6);
    for _ in 0..7 {
        put(&mut state, PlayerId::P1, "Island", Zone::Graveyard);
    }
    assert!(state.stack.is_empty());
    assert!(state.engine.pending_triggers.is_empty());
    drain(&mut state, None);
    assert!(scions(&state).is_empty());
}

#[test]
fn attack_with_seven_cards_offers_a_real_refusal() {
    let (mut state, _) = attack(7);
    reach_option(&mut state);
    engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap();
    drain(&mut state, None);
    assert!(scions(&state).is_empty());
}

#[test]
fn accepted_threshold_creates_one_exact_scion_and_restores_optional_choice() {
    let (mut state, _) = attack(7);
    reach_option(&mut state);
    let saved = serde_json::to_vec(&state).unwrap();
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    drain(&mut state, None);
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    engine::step(&mut restored, Action::ChooseEffectOption(1)).unwrap();
    drain(&mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
    let created = scions(&state);
    assert_eq!(created.len(), 1);
    let token = state.objects.get(created[0]);
    assert_eq!(token.name, "Scion of the Deep");
    assert_eq!(
        (token.owner, token.controller),
        (PlayerId::P0, PlayerId::P0)
    );
    assert_eq!(
        (
            engine::effective_power(&state, created[0]),
            engine::effective_toughness(&state, created[0])
        ),
        (8, 8)
    );
}

#[test]
fn losing_threshold_before_resolution_suppresses_the_optional_choice() {
    let (mut state, _) = attack(7);
    assert!(!state.stack.is_empty());
    let card = state.players[0].graveyard[0];
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Hand));
    assert_eq!(state.players[0].graveyard.len(), 6);
    drain(&mut state, None);
    assert!(scions(&state).is_empty());
}

#[test]
fn reaching_threshold_after_an_ineligible_attack_does_not_create_a_trigger() {
    let (mut state, _) = attack(6);
    put(&mut state, PlayerId::P0, "Island", Zone::Graveyard);
    drain(&mut state, None);
    assert!(state.stack.is_empty());
    assert!(scions(&state).is_empty());
}

#[test]
fn departed_kiora_keeps_its_queued_trigger_controller_and_threshold_check() {
    let (mut state, kiora) = attack(7);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(kiora, Zone::Hand));
    reach_option(&mut state);
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    drain(&mut state, None);
    assert_eq!(state.objects.get(kiora).zone, Zone::Hand);
    assert_eq!(scions(&state).len(), 1);
}

#[test]
fn second_scion_uses_the_legend_rule_and_restores_that_decision() {
    let (mut state, _) = attack(7);
    let old = put(
        &mut state,
        PlayerId::P0,
        "Scion of the Deep Token",
        Zone::Battlefield,
    );
    reach_option(&mut state);
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    let Decision::ChooseLegendPermanent { player, candidates } = next(&mut state) else {
        panic!("Scion legend decision")
    };
    assert_eq!(player, PlayerId::P0);
    assert_eq!(candidates.len(), 2);
    assert!(candidates.contains(&old));
    let saved = serde_json::to_vec(&state).unwrap();
    engine::step(&mut state, Action::ChooseLegendPermanent(old)).unwrap();
    drain(&mut state, None);
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    engine::step(&mut restored, Action::ChooseLegendPermanent(old)).unwrap();
    drain(&mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(scions(&state), vec![old]);
}
