//! Source preparation. Card registration and gameplay qualification are pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{
    GameObject, GameState, ObjectStateV4, StackTargetContractV4, Step, Target, Zone,
};
use mtg_kernel::trigger;

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        837,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def =
        card_id_by_name(name).unwrap_or_else(|| panic!("unregistered fixture card: {name}"));
    let id = state.objects.push(GameObject {
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
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        _ => panic!("unsupported fixture zone"),
    }
    id
}

fn enter(state: &mut GameState, player: PlayerId) -> ObjectId {
    let source = put(state, player, "Mischievous Pup", Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
    let pending = trigger::collect_and_process(state);
    assert_eq!(pending.len(), 1);
    state.engine.pending_triggers.extend(pending);
    source
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn targets(state: &mut GameState) -> Decision {
    for _ in 0..32 {
        match next(state) {
            choice @ Decision::ChooseTargets { .. } => return choice,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected decision {other:?}"),
        }
    }
    panic!("Pup target choice absent");
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseTargets {
                can_finish: true, ..
            } => engine::step(state, Action::FinishEffectSelection).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected decision {other:?}"),
        }
    }
    panic!("Pup resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn refuse(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

#[test]
fn exact_metadata_colored_payment_and_opponent_turn_flash_for_both_seats() {
    let id = card_id_by_name("Mischievous Pup").unwrap();
    assert_eq!(id, 365);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Dog]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(3), Some(1), 3)
    );
    assert_eq!(def.cost.generic, 2);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert!(def.keywords.has(Keywords::FLASH));
    assert_eq!(
        trigger::trigger_target_spec(id),
        TargetSpec::UpToOneOtherControlledPermanent
    );
    assert_eq!(TargetSpec::UpToOneOtherControlledPermanent.stable_id(), 58);
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player.opponent());
        state.step = Step::End;
        let source = put(&mut state, player, "Mischievous Pup", Zone::Hand);
        next(&mut state);
        engine::step(&mut state, Action::Pass).unwrap();
        state.players[player.index()].mana_pool[5] = 3;
        next(&mut state);
        refuse(&mut state, Action::CastSpell(source));
        state.players[player.index()].mana_pool[5] = 2;
        state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        let Decision::CastSpellOrPass {
            castable_spells,
            player: chooser,
            ..
        } = next(&mut state)
        else {
            panic!("opponent-turn priority absent");
        };
        assert_eq!(chooser, player);
        assert!(castable_spells.contains(&source));
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            let Decision::ChooseTargets {
                legal_targets,
                can_finish,
                ..
            } = targets(current)
            else {
                unreachable!();
            };
            assert!(legal_targets.is_empty() && can_finish);
            engine::step(current, Action::FinishEffectSelection).unwrap();
            settle(current);
            assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}

#[test]
fn any_other_controlled_permanent_including_token_or_borrowed_card_can_return() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for name in [
            "Forest",
            "Llanowar Elves",
            "Lembas",
            "Journey to Nowhere",
            "Ajani, Caller of the Pride",
            "Rat Token",
        ] {
            let mut state = ready(player);
            let wanted = if name == "Ajani, Caller of the Pride" {
                let id = put(&mut state, player, name, Zone::Hand);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(id, Zone::Battlefield),
                );
                id
            } else {
                put(&mut state, player, name, Zone::Battlefield)
            };
            let foreign = put(&mut state, player.opponent(), "Forest", Zone::Battlefield);
            let hand = put(&mut state, player, "Forest", Zone::Hand);
            let grave = put(&mut state, player, "Forest", Zone::Graveyard);
            let source = enter(&mut state, player);
            let Decision::ChooseTargets {
                legal_targets,
                remaining,
                can_finish,
                ..
            } = targets(&mut state)
            else {
                unreachable!();
            };
            assert_eq!((remaining, can_finish), (1, true));
            assert_eq!(legal_targets, vec![Target::Object(wanted)]);
            for excluded in [source, foreign, hand, grave] {
                refuse(&mut state, Action::ChooseTarget(Target::Object(excluded)));
            }
            engine::step(&mut state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
            refuse(&mut state, Action::ChooseTarget(Target::Object(foreign)));
            settle(&mut state);
            assert_eq!(state.objects.get(wanted).zone, Zone::Hand);
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            // Tokens disappear at the next SBA instead of becoming a hand card.
            assert_eq!(
                state.players[player.index()].hand.contains(&wanted),
                name != "Rat Token"
            );
        }
        let mut state = ready(player);
        let borrowed = put(&mut state, player.opponent(), "Forest", Zone::Battlefield);
        change_controller(&mut state, borrowed, player);
        enter(&mut state, player);
        targets(&mut state);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(borrowed))).unwrap();
        settle(&mut state);
        assert!(state.players[player.opponent().index()]
            .hand
            .contains(&borrowed));
        assert!(!state.players[player.index()].hand.contains(&borrowed));
    }
}

fn change_controller(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    for player in &mut state.players {
        player.battlefield.retain(|&id| id != object);
    }
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}

#[test]
fn zero_or_one_choice_and_pending_or_finished_stack_restore_identically() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let mut state = ready(player);
            let wanted = put(&mut state, player, "Forest", Zone::Battlefield);
            enter(&mut state, player);
            let decision = targets(&mut state);
            let mut replay = restored(&state);
            assert_eq!(next(&mut replay), decision);
            let action = if accept {
                Action::ChooseTarget(Target::Object(wanted))
            } else {
                Action::FinishEffectSelection
            };
            engine::step(&mut state, action.clone()).unwrap();
            engine::step(&mut replay, action).unwrap();
            let mut answered = restored(&state);
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert!(state.engine.pending_triggers.is_empty() && state.stack.len() == 1);
            let mut stacked = restored(&state);
            for current in [&mut state, &mut replay, &mut answered, &mut stacked] {
                settle(current);
                assert_eq!(
                    current.objects.get(wanted).zone,
                    if accept {
                        Zone::Hand
                    } else {
                        Zone::Battlefield
                    }
                );
            }
            for other in [&replay, &answered, &stacked] {
                assert_eq!(state.diagnostic_state_hash(), other.diagnostic_state_hash());
            }
        }
    }
}

#[test]
fn captured_source_exclusion_allows_the_returned_source_as_a_new_permanent() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = enter(&mut state, player);
        targets(&mut state);
        refuse(&mut state, Action::ChooseTarget(Target::Object(source)));
        let original = state.engine.pending_triggers[0].source_contract.unwrap();
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        // Isolate the old ability while consuming the separate new ETB event.
        let new_etb = trigger::collect_and_process(&mut state);
        assert_eq!(new_etb.len(), 1);
        assert_ne!(original, new_etb[0].source_contract.unwrap());
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            unreachable!();
        };
        assert_eq!(legal_targets, vec![Target::Object(source)]);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseTarget(Target::Object(source))).unwrap();
            settle(current);
            assert_eq!(current.objects.get(source).zone, Zone::Hand);
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            replay.diagnostic_state_hash()
        );
    }
}

#[test]
fn restored_illegal_original_source_prefix_is_rejected_before_stack_placement() {
    let mut state = ready(PlayerId::P0);
    let source = enter(&mut state, PlayerId::P0);
    targets(&mut state);
    let target = Target::Object(source);
    let contract = StackTargetContractV4::capture(&state, target);
    state.engine.pending_triggers[0].targets = vec![target];
    state.engine.pending_triggers[0].target_contracts = vec![contract];
    let mut replay = restored(&state);
    assert!(matches!(
        engine::advance_until_decision(&mut replay),
        Decision::Halted { .. }
    ));
    assert!(replay.stack.is_empty());
    assert_eq!(replay.objects.get(source).zone, Zone::Battlefield);
}

#[test]
fn captured_trigger_survives_source_departure_but_rechecks_control_and_target_incarnation() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for change in 0..4 {
            let mut state = ready(player);
            let wanted = put(&mut state, player, "Forest", Zone::Battlefield);
            let source = enter(&mut state, player);
            targets(&mut state);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
            match change {
                0 => {}
                1 => change_controller(&mut state, wanted, player.opponent()),
                2 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(wanted, Zone::Exile),
                    );
                }
                3 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(wanted, Zone::Exile),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(wanted, Zone::Battlefield),
                    );
                }
                _ => unreachable!(),
            }
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(
                    current.objects.get(wanted).zone,
                    match change {
                        0 => Zone::Hand,
                        2 => Zone::Exile,
                        _ => Zone::Battlefield,
                    }
                );
                assert_eq!(current.objects.get(source).zone, Zone::Exile);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                replay.diagnostic_state_hash()
            );
        }
    }
}
