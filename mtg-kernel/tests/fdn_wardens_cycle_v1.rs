//! Prepared morbid modes; catalog admission and gameplay qualification remain pending.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::trigger;

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        836,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
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
        Zone::Hand => state.players[player.index()].hand.push(object),
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        _ => panic!("fixture zone"),
    }
    object
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn pass(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected decision {other:?}"),
    };
    engine::step(state, action).unwrap();
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        pass(state, decision);
    }
    panic!("resolution did not settle");
}

fn end(state: &mut GameState) -> Decision {
    state.step = Step::Main2;
    for _ in 0..32 {
        let decision = next(state);
        if state.step == Step::End {
            return decision;
        }
        pass(state, decision);
    }
    panic!("end step did not begin");
}

fn death(state: &mut GameState, player: PlayerId) {
    let victim = put(state, player, "Elvish Mystic", Zone::Battlefield);
    event::propose_and_commit(state, ProposedEvent::zone_change(victim, Zone::Graveyard));
    assert!(state.creature_died_this_turn_v1());
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn printed_definition_and_actual_payment_are_exact_for_both_seats() {
    let card = card_id_by_name("Wardens of the Cycle").unwrap();
    assert_eq!(card, 361);
    let def = &CARD_DEFS[card as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(3), Some(4), 4)
    );
    assert_eq!(def.subtypes, &[Subtype::Elf, Subtype::Warlock]);
    assert_eq!(def.cost.generic, 1);
    assert_eq!(
        def.cost.pips,
        &[
            Pip::Colored(ManaColor::B),
            Pip::Colored(ManaColor::G),
            Pip::Colored(ManaColor::G)
        ]
    );
    assert_eq!(trigger::triggers_for(card).len(), 1);
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Wardens of the Cycle", Zone::Hand);
        let mana = &mut state.players[player.index()].mana_pool;
        mana[5] = 1;
        mana[ManaColor::B.pool_index()] = 1;
        mana[ManaColor::G.pool_index()] = 1;
        next(&mut state);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state.players[player.index()].mana_pool[ManaColor::G.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
    }
}

#[test]
fn no_death_and_opponents_end_step_do_not_offer_modes() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for opposing in [false, true] {
            let mut state = ready(player);
            put(
                &mut state,
                if opposing { player.opponent() } else { player },
                "Wardens of the Cycle",
                Zone::Battlefield,
            );
            if opposing {
                death(&mut state, player);
            }
            assert!(matches!(end(&mut state), Decision::CastSpellOrPass { .. }));
            assert!(state.stack.is_empty());
            assert!(state.engine.pending_triggers.is_empty());
        }
    }
}

#[test]
fn both_modes_restore_before_choice_and_after_placement_with_source_gone() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for victim_owner in [player, player.opponent()] {
            for mode in [0, 1] {
                let mut state = ready(player);
                death(&mut state, victim_owner);
                let source = put(
                    &mut state,
                    player,
                    "Wardens of the Cycle",
                    Zone::Battlefield,
                );
                assert!(matches!(
                    end(&mut state),
                    Decision::ChooseTriggerMode { .. }
                ));
                let mut before = copy(&state);
                for game in [&mut state, &mut before] {
                    engine::step(game, Action::ChooseTriggerMode(mode)).unwrap();
                    assert!(matches!(next(game), Decision::CastSpellOrPass { .. }));
                    assert!(game.engine.pending_triggers.is_empty());
                    assert_eq!(game.stack.len(), 1);
                    assert!(game.stack.last().unwrap().targets.is_empty());
                    event::propose_and_commit(
                        game,
                        ProposedEvent::zone_change(source, Zone::Exile),
                    );
                }
                let mut placed = copy(&state);
                for game in [&mut state, &mut before, &mut placed] {
                    settle(game);
                    assert_eq!(
                        game.players[player.index()].life,
                        if mode == 0 { 22 } else { 19 }
                    );
                    assert_eq!(
                        game.players[player.index()].hand.len(),
                        usize::from(mode == 1)
                    );
                    assert_eq!(game.players[player.opponent().index()].life, 20);
                    assert!(game.players[player.opponent().index()].hand.is_empty());
                }
                assert_eq!(state.state_hash(), before.state_hash());
                assert_eq!(
                    state.diagnostic_state_hash(),
                    placed.diagnostic_state_hash()
                );
            }
        }
    }
}

#[test]
fn each_selected_branch_rechecks_morbid_on_resolution() {
    for mode in [0, 1] {
        let mut state = ready(PlayerId::P0);
        put(
            &mut state,
            PlayerId::P0,
            "Wardens of the Cycle",
            Zone::Battlefield,
        );
        death(&mut state, PlayerId::P1);
        assert!(matches!(
            end(&mut state),
            Decision::ChooseTriggerMode { .. }
        ));
        engine::step(&mut state, Action::ChooseTriggerMode(mode)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        // Synthetic next-turn ledger boundary isolates the intervening-if recheck.
        state.turn += 1;
        assert!(!state.creature_died_this_turn_v1());
        settle(&mut state);
        assert_eq!(state.players[0].life, 20);
        assert!(state.players[0].hand.is_empty());
    }
}

#[test]
fn multiple_deaths_offer_one_choice_and_late_death_does_not_retroactively_trigger() {
    let mut state = ready(PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Wardens of the Cycle",
        Zone::Battlefield,
    );
    death(&mut state, PlayerId::P0);
    death(&mut state, PlayerId::P1);
    assert!(matches!(
        end(&mut state),
        Decision::ChooseTriggerMode { .. }
    ));
    engine::step(&mut state, Action::ChooseTriggerMode(0)).unwrap();
    settle(&mut state);
    assert_eq!(state.players[0].life, 22);
    next(&mut state);
    assert!(state.stack.is_empty());
    assert!(state.engine.pending_triggers.is_empty());

    let mut state = ready(PlayerId::P0);
    put(
        &mut state,
        PlayerId::P0,
        "Wardens of the Cycle",
        Zone::Battlefield,
    );
    assert!(matches!(end(&mut state), Decision::CastSpellOrPass { .. }));
    death(&mut state, PlayerId::P1);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.is_empty());
    assert!(state.engine.pending_triggers.is_empty());
}
