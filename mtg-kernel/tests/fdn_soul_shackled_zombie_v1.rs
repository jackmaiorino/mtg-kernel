//! Future printed acceptance. Requires actual FDN registration, without a
//! synthetic definition, catalog pin, conditional skip or admission bypass.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Subtype, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{
    GameObject, GameState, ObjectStateV4, StackTargetContractV4, Step, Target, Zone,
};

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_309,
        actor,
    );
    state.step = Step::Main1;
    state
}
fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("requires actual FDN admission");
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Hand,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[owner.index()].hand.push(id);
    if zone != Zone::Hand {
        event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    }
    id
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn settle(state: &mut GameState) {
    for _ in 0..60 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("did not settle");
}
fn cast_to_etb(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Soul-Shackled Zombie", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 4;
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    for _ in 0..30 {
        match next(state) {
            Decision::ChooseTargets {
                player,
                spell,
                remaining: 2,
                can_finish: true,
                ..
            } if spell == source && player == actor => {
                assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
                assert_eq!(
                    state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
                    0
                );
                return source;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("did not reach actual ETB");
}
fn finish_targets(state: &mut GameState, targets: &[ObjectId]) {
    for &target in targets {
        assert!(
            matches!(next(state),Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
        *state = restore(state);
    }
    if targets.len() < 2 {
        assert!(matches!(
            next(state),
            Decision::ChooseTargets {
                can_finish: true,
                ..
            }
        ));
        engine::step(state, Action::FinishEffectSelection).unwrap();
    }
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(
        state.stack.last().unwrap().inline_effect,
        Some(EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1)
    );
    *state = restore(state);
}
fn life(state: &GameState, actor: PlayerId) -> (i32, i32) {
    (
        state.players[actor.index()].life,
        state.players[actor.opponent().index()].life,
    )
}
fn readonly_refusal(mut state: GameState, action: Action) {
    let before = serde_json::to_vec(&state).unwrap();
    assert!(engine::step(&mut state, action).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}
fn control(state: &mut GameState, source: ObjectId, controller: PlayerId) {
    // Physical accepted-controller primitive, not an invented printed producer.
    for player in [PlayerId::P0, PlayerId::P1] {
        state.players[player.index()]
            .battlefield
            .retain(|id| *id != source);
    }
    state.players[controller.index()].battlefield.push(source);
    state.objects.get_mut(source).controller = controller;
}

#[test]
fn printed_definition_real_cast_and_zero_one_two_optional_targets_both_seats() {
    let def = &CARD_DEFS
        [card_id_by_name("Soul-Shackled Zombie").expect("future FDN registration") as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(4), Some(2), 4)
    );
    assert_eq!(def.cost.generic, 3);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert_eq!(def.subtypes, &[Subtype::Zombie]);
    assert_eq!(TargetSpec::UpToTwoCardsInSingleGraveyard.stable_id(), 66);
    for actor in [PlayerId::P0, PlayerId::P1] {
        for names in [
            vec![],
            vec!["Forest"],
            vec!["Llanowar Elves"],
            vec!["Llanowar Elves", "Forest"],
            vec!["Llanowar Elves", "Llanowar Elves"],
        ] {
            let mut state = ready(actor);
            let cards = names
                .iter()
                .map(|name| put(&mut state, actor.opponent(), name, Zone::Graveyard))
                .collect::<Vec<_>>();
            let source = cast_to_etb(&mut state, actor);
            assert!(state.objects.get(source).summoning_sick);
            let initial = life(&state, actor);
            finish_targets(&mut state, &cards);
            settle(&mut state);
            assert!(cards
                .iter()
                .all(|card| state.objects.get(*card).zone == Zone::Exile));
            let drain = if names.contains(&"Llanowar Elves") {
                2
            } else {
                0
            };
            assert_eq!(life(&state, actor), (initial.0 + drain, initial.1 - drain));
        }
    }
}

#[test]
fn one_physical_graveyard_distinct_cards_and_readonly_pending_refusals() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let first = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let same = put(&mut state, actor, "Forest", Zone::Graveyard);
        let other = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Graveyard,
        );
        let source = cast_to_etb(&mut state, actor);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(first))).unwrap();
        state = restore(&state);
        assert!(
            matches!(next(&mut state),Decision::ChooseTargets { legal_targets, can_finish: true, remaining: 1, .. }
            if legal_targets.contains(&Target::Object(same))
                && !legal_targets.contains(&Target::Object(first))
                && !legal_targets.contains(&Target::Object(other)))
        );
        readonly_refusal(state.clone(), Action::ChooseTarget(Target::Object(first)));
        readonly_refusal(state.clone(), Action::ChooseTarget(Target::Object(other)));
        let original_history = state.engine.event_history.clone();
        for corrupt in 0..5 {
            let mut forged = restore(&state);
            let pending = &mut forged.engine.pending_triggers[0];
            match corrupt {
                0 => pending.target_spec = TargetSpec::UpToTwoCardsInGraveyards,
                1 => pending.controller = actor.opponent(),
                2 => pending.source_contract.as_mut().unwrap().zone_change_count += 1,
                3 => pending.target_contracts.clear(),
                4 => pending.source = first,
                _ => unreachable!(),
            }
            assert_eq!(forged.engine.event_history, original_history);
            readonly_refusal(forged, Action::FinishEffectSelection);
        }
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(first).zone, Zone::Exile);
        assert_eq!(state.objects.get(same).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(other).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    }
}

#[test]
fn actual_response_exiles_one_target_partial_resolution_counts_only_success() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for remaining_is_creature in [false, true] {
            let mut state = ready(actor);
            let removed = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            let remaining = put(
                &mut state,
                actor,
                if remaining_is_creature {
                    "Llanowar Elves"
                } else {
                    "Forest"
                },
                Zone::Graveyard,
            );
            let _source = cast_to_etb(&mut state, actor);
            finish_targets(&mut state, &[removed, remaining]);
            let initial = life(&state, actor);
            let responder = actor.opponent();
            let faerie = put(&mut state, responder, "Faerie Macabre", Zone::Hand);
            // Advance ordinary priority until the responder can activate its
            // real hand ability over the actual ETB stack item.
            if state.priority_player != responder {
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert!(
                matches!(next(&mut state),Decision::CastSpellOrPass { player, activatable_abilities, .. }
                if player == responder && activatable_abilities.contains(&(faerie, 0)))
            );
            engine::step(&mut state, Action::ActivateAbility(faerie, 0)).unwrap();
            assert!(
                matches!(next(&mut state),Decision::ChooseEffectTargets { legal_targets, can_finish: true, .. }
                if legal_targets.contains(&Target::Object(removed)))
            );
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(removed)),
            )
            .unwrap();
            next(&mut state);
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
            state = restore(&state);
            settle(&mut state);
            assert_eq!(state.objects.get(faerie).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(removed).zone, Zone::Exile);
            assert_eq!(state.objects.get(remaining).zone, Zone::Exile);
            let drain = if remaining_is_creature { 2 } else { 0 };
            assert_eq!(life(&state, actor), (initial.0 + drain, initial.1 - drain));
        }
    }
}

#[test]
fn departed_and_reentered_targets_are_stale_all_illegal_fizzles_without_gain() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for both_leave in [false, true] {
            let mut state = ready(actor);
            let creature = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Graveyard,
            );
            let land = put(&mut state, actor.opponent(), "Forest", Zone::Graveyard);
            cast_to_etb(&mut state, actor);
            finish_targets(&mut state, &[creature, land]);
            let initial = life(&state, actor);
            for card in if both_leave {
                vec![creature, land]
            } else {
                vec![creature]
            } {
                event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Hand));
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(card, Zone::Graveyard),
                );
            }
            state = restore(&state);
            settle(&mut state);
            assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
            assert_eq!(
                state.objects.get(land).zone,
                if both_leave {
                    Zone::Graveyard
                } else {
                    Zone::Exile
                }
            );
            assert_eq!(life(&state, actor), initial);
        }
    }
}

#[test]
fn actual_entry_controller_is_retained_and_coherent_rewrites_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for depart in [false, true] {
            let mut state = ready(actor);
            let card = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Graveyard,
            );
            let source = cast_to_etb(&mut state, actor);
            control(&mut state, source, actor.opponent());
            let mut forged = restore(&state);
            forged.engine.pending_triggers[0].controller = actor.opponent();
            forged.engine.pending_triggers[0]
                .source_contract
                .as_mut()
                .unwrap()
                .controller = actor.opponent();
            assert_eq!(forged.engine.event_history, state.engine.event_history);
            readonly_refusal(forged, Action::FinishEffectSelection);
            finish_targets(&mut state, &[card]);
            if depart {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Hand),
                );
            }
            state = restore(&state);
            let mut forged = state.clone();
            let item = forged.stack.last_mut().unwrap();
            item.controller = actor.opponent();
            item.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
            readonly_refusal(forged, Action::Pass);
            let initial = life(&state, actor);
            settle(&mut state);
            assert_eq!(life(&state, actor), (initial.0 + 2, initial.1 - 2));
            assert_eq!(state.objects.get(card).zone, Zone::Exile);
        }
    }
}

#[test]
fn finalized_contract_shape_and_entry_receipt_forgery_refuse_after_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let one = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let two = put(&mut state, actor, "Forest", Zone::Graveyard);
        let other = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Graveyard,
        );
        cast_to_etb(&mut state, actor);
        finish_targets(&mut state, &[one, two]);
        for corrupt in 0..6 {
            let mut forged = restore(&state);
            let other_contract = StackTargetContractV4::capture(&forged, Target::Object(other));
            let item = forged.stack.last_mut().unwrap();
            match corrupt {
                0 => {
                    item.targets[1] = Target::Object(other);
                    item.v4.target_contracts[1] = other_contract;
                }
                1 => {
                    item.targets[1] = item.targets[0];
                    item.v4.target_contracts[1] = item.v4.target_contracts[0];
                }
                2 => item.v4.target_spec = Some(TargetSpec::UpToTwoCardsInGraveyards),
                3 => {
                    if let StackTargetContractV4::Object {
                        zone_change_count, ..
                    } = &mut item.v4.target_contracts[0]
                    {
                        *zone_change_count += 1;
                    }
                }
                4 => forged.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::SingleGraveyardEntryV1 { .. })
                }),
                5 => {
                    let receipt = forged
                        .engine
                        .event_history
                        .iter()
                        .find(|event| {
                            matches!(event, CommittedEvent::SingleGraveyardEntryV1 { .. })
                        })
                        .unwrap()
                        .clone();
                    forged.engine.event_log.push(receipt);
                }
                _ => unreachable!(),
            }
            readonly_refusal(forged, Action::Pass);
        }
    }
}
