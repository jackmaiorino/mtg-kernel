//! Future actual FDN registration acceptance. No fallback definition or ID.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("actual serially registered definition required");
    let object = state.objects.push(GameObject {
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
    state.players[owner.index()].hand.push(object);
    if zone != Zone::Hand {
        event::propose_and_commit(state, ProposedEvent::zone_change(object, zone));
    }
    object
}
fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_330,
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
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn settle(state: &mut GameState) {
    for _ in 0..80 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected resolution {other:?}"),
        }
    }
    panic!("resolution did not complete");
}
fn main_for(state: &mut GameState, actor: PlayerId) {
    for _ in 0..150 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.active_player == actor
                    && state.step == Step::Main1
                    && state.stack.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { .. } => {
                engine::step(state, Action::DeclareAttackers(vec![])).unwrap()
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            other => panic!("unexpected real turn progression {other:?}"),
        }
    }
    panic!("did not reach main phase");
}
fn cast(state: &mut GameState, actor: PlayerId, name: &str, color: ManaColor) -> ObjectId {
    let source = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool[color.pool_index()] = 1;
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(
        state.players[actor.index()].mana_pool[color.pool_index()],
        0
    );
    source
}
fn until_gain(state: &mut GameState) {
    for _ in 0..60 {
        let decision = next(state);
        if state.stack.iter().any(|item| {
            matches!(
                item.inline_effect,
                Some(EffectOp::AuthorityEntryGainLifeV1 { .. })
            )
        }) {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected creature entry {other:?}"),
        }
    }
    panic!("actual opposing entry gain missing");
}
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn transfer(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let previous = state.objects.get(object).controller;
    state.players[previous.index()]
        .battlefield
        .retain(|id| *id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}
fn readonly_pass(state: &mut GameState) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, Action::Pass).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

#[test]
fn actual_white_enchantment_own_entry_negative_and_opposing_real_cast_tapped_gain_both_seats() {
    let def = &CARD_DEFS
        [card_id_by_name("Authority of the Consuls").expect("future FDN registration") as usize];
    assert_eq!(def.cost.generic, 0);
    assert_eq!(def.cost.pips, [Pip::Colored(ManaColor::W)]);
    assert_eq!(def.types, [CardType::Enchantment]);
    assert_eq!(def.target_spec, TargetSpec::None);
    assert!((def.spell_effect)().is_none());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let authority = cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
        settle(&mut state);
        assert_eq!(state.objects.get(authority).zone, Zone::Battlefield);
        let own = cast(&mut state, actor, "Llanowar Elves", ManaColor::G);
        settle(&mut state);
        assert!(!state.objects.get(own).tapped);
        assert_eq!(state.players[actor.index()].life, 20);
        main_for(&mut state, actor.opponent());
        let opposing = cast(&mut state, actor.opponent(), "Llanowar Elves", ManaColor::G);
        until_gain(&mut state);
        assert!(state.objects.get(opposing).tapped);
        assert_eq!(state.players[actor.index()].life, 20);
        assert_eq!(state.stack.last().unwrap().controller, actor);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.players[actor.index()].life, 21);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn future_actual_static_and_trigger_meanings_bind_distinct_complete_roots_and_refuse_forgery() {
    use mtg_kernel::rules_meaning_authority_v1 as meaning;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
        settle(&mut state);
        let id = state.objects.get(source).card_def;
        let selector = meaning::live_selector_v1().unwrap();
        let static_meaning = meaning::meaning_v1(
            &selector,
            id,
            meaning::RootV1::OpponentCreatureEntryTappedReplacement,
        )
        .unwrap();
        let trigger_meaning = meaning::meaning_v1(
            &selector,
            id,
            meaning::RootV1::OpponentCreatureEntryGainLifeTrigger,
        )
        .unwrap();
        assert_ne!(static_meaning.root_sha256, trigger_meaning.root_sha256);
        assert_eq!(static_meaning.trigger_index, None);
        assert_eq!(trigger_meaning.trigger_index, Some(0));
        let entrant = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        assert!(state.objects.get(entrant).tapped);
        until_gain(&mut state);
        assert!(matches!(
            state.stack.last().unwrap().inline_effect,
            Some(EffectOp::AuthorityEntryGainLifeV1 { .. })
        ));
        let before = serde_json::to_vec(&state).unwrap();
        for record in [&static_meaning, &trigger_meaning] {
            let roundtrip = serde_json::from_slice(&serde_json::to_vec(record).unwrap()).unwrap();
            meaning::validate_meaning_v1(&roundtrip).unwrap();
            let mut extra = serde_json::to_value(record).unwrap();
            extra["unknown_authority"] = true.into();
            assert!(serde_json::from_value::<meaning::CardMeaningV1>(extra).is_err());
            let mut extra_fact = serde_json::to_value(record).unwrap();
            extra_fact["facts"][0]["ActivePrintedBattlefieldFace"]["unknown_authority"] =
                true.into();
            assert!(serde_json::from_value::<meaning::CardMeaningV1>(extra_fact).is_err());
            assert!(!record.public_human_sampler_and_model_admitted);
            for field in 0..13 {
                let mut forged = record.clone();
                match field {
                    0 => forged.selector.schema.push('x'),
                    1 => forged.selector.scope.push('x'),
                    2 => forged.selector.card_db_hash ^= 1,
                    3 => forged.selector.producer_sha256.push('x'),
                    4 => forged.card_id = card_id_by_name("Lightning Bolt").unwrap(),
                    5 => forged.printed_name.push('x'),
                    6 => forged.printed_cost = "{0}".into(),
                    7 => forged.printed_mana_value = 2,
                    8 => forged.ordinary_untargeted_enchantment_spell = false,
                    9 => {
                        forged.root = if record.root
                            == meaning::RootV1::OpponentCreatureEntryTappedReplacement
                        {
                            meaning::RootV1::OpponentCreatureEntryGainLifeTrigger
                        } else {
                            meaning::RootV1::OpponentCreatureEntryTappedReplacement
                        }
                    }
                    10 => forged.trigger_index = Some(1),
                    11 => forged.root_sha256.push('x'),
                    _ => forged.public_human_sampler_and_model_admitted = true,
                }
                assert!(
                    meaning::validate_meaning_v1(&forged).is_err(),
                    "field={field}"
                );
            }
            for index in 0..record.facts.len() {
                let mut omitted = record.clone();
                omitted.facts.remove(index);
                assert!(meaning::validate_meaning_v1(&omitted).is_err());
                let mut repeated = record.clone();
                repeated.facts.push(record.facts[index].clone());
                assert!(meaning::validate_meaning_v1(&repeated).is_err());
                let mut reordered = record.clone();
                reordered
                    .facts
                    .swap(index, (index + 1) % record.facts.len());
                assert!(meaning::validate_meaning_v1(&reordered).is_err());
                let mut rewritten = record.clone();
                rewritten.facts[index] =
                    meaning::FactV1::ActivePrintedBattlefieldFace { face_index: 1 };
                assert!(meaning::validate_meaning_v1(&rewritten).is_err());
            }
        }
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let mut replay = restored(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.players[actor.index()].life, 21);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn original_observer_controller_and_physical_entrant_owner_survive_response_and_reentry() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for response in 0..4 {
            let mut state = ready(actor);
            let authority = cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
            settle(&mut state);
            // Genuine physical entry primitive: the creature's owner differs
            // from its actual entering controller, which defines "opponent".
            let entrant = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change_to_battlefield_under_controller(
                    entrant,
                    actor.opponent(),
                ),
            );
            until_gain(&mut state);
            assert_eq!(state.objects.get(entrant).owner, actor);
            assert!(state.objects.get(entrant).tapped);
            match response {
                0 => transfer(&mut state, authority, actor.opponent()),
                1 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(authority, Zone::Hand),
                ),
                2 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(authority, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(authority, Zone::Battlefield),
                    );
                }
                _ => {
                    transfer(&mut state, entrant, actor);
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(entrant, Zone::Hand),
                    );
                }
            }
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(current.players[actor.index()].life, 21);
                assert_eq!(current.players[actor.opponent().index()].life, 20);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn restored_captured_beneficiary_source_and_collection_forgery_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
        settle(&mut state);
        put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        until_gain(&mut state);
        for corruption in 0..5 {
            let mut forged = restored(&state);
            match corruption {
                0 => {
                    let item = forged.stack.last_mut().unwrap();
                    item.controller = actor.opponent();
                    item.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
                }
                1 => {
                    forged
                        .stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                2 => forged.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::AuthorityEntryCollectedV1 { .. })
                }),
                3 => {
                    let receipt = forged
                        .engine
                        .event_history
                        .iter()
                        .find(|event| {
                            matches!(event, CommittedEvent::AuthorityEntryCollectedV1 { .. })
                        })
                        .unwrap()
                        .clone();
                    forged.engine.event_history.push(receipt);
                }
                _ => {
                    forged.stack.last_mut().unwrap().inline_effect =
                        Some(EffectOp::AuthorityEntryGainLifeV1 {
                            capture_history_index: 0,
                        })
                }
            }
            readonly_pass(&mut forged);
            let mut repeated = restored(&forged);
            readonly_pass(&mut repeated);
        }
        let surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
        let before = serde_json::to_vec(&state).unwrap();
        for viewer in [actor, actor.opponent()] {
            assert!(
                mtg_kernel::rl::observe_policy_v8(&state, &surface, viewer, 0, 0, 0, 1).is_err()
            );
            let observation = mtg_kernel::rl::observe_v1(&state, viewer, 0).unwrap();
            assert!(observation.projection.battlefield[actor.index()]
                .iter()
                .any(|card| card.card_name == "Authority of the Consuls"));
        }
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[test]
fn independent_entry_observers_reject_coherent_beneficiary_and_source_rewrites_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let authority = cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
        settle(&mut state);
        let entrant = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(entrant, actor.opponent()),
        );
        until_gain(&mut state);
        transfer(&mut state, authority, actor.opponent());
        transfer(&mut state, entrant, actor);
        // Current control no longer reveals either original entry controller.
        // The authentic continuation still gains life for the original observer.
        let mut valid = restored(&state);
        settle(&mut valid);
        assert_eq!(valid.players[actor.index()].life, 21);
        assert_eq!(valid.players[actor.opponent().index()].life, 20);
        let observers: Vec<_> = state
            .engine
            .event_history
            .iter()
            .filter(|event| matches!(event, CommittedEvent::AuthorityEntryObserversV1 { .. }))
            .cloned()
            .collect();
        assert_eq!(observers.len(), 1);
        for corruption in 0..6 {
            let mut forged = restored(&state);
            if corruption < 4 {
                let capture = forged
                    .engine
                    .event_history
                    .iter_mut()
                    .find_map(|event| match event {
                        CommittedEvent::AuthorityEntryCaptureV1 { capture } => Some(capture),
                        _ => None,
                    })
                    .unwrap();
                match corruption {
                    0 => {
                        capture.source.controller = actor.opponent();
                        capture.entrant_controller = actor;
                    }
                    1 => capture.source.owner = actor.opponent(),
                    2 => capture.source.card_def = card_id_by_name("Llanowar Elves").unwrap(),
                    _ => capture.source.zone_change_count += 1,
                }
                let source = capture.source;
                let item = forged.stack.last_mut().unwrap();
                item.controller = source.controller;
                item.v4.ability_source_contract = Some(source);
            } else if corruption == 4 {
                // The independent observer row also requires its individual
                // capture, even after the mutable dispatch rows disappear.
                forged.engine.event_history.retain(|event| {
                    !matches!(
                        event,
                        CommittedEvent::AuthorityEntryCaptureV1 { .. }
                            | CommittedEvent::AuthorityEntryCollectedV1 { .. }
                    )
                });
            } else {
                forged.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::AuthorityEntryObserversV1 { .. })
                });
            }
            if corruption != 5 {
                assert_eq!(
                    forged
                        .engine
                        .event_history
                        .iter()
                        .filter(|event| {
                            matches!(event, CommittedEvent::AuthorityEntryObserversV1 { .. })
                        })
                        .cloned()
                        .collect::<Vec<_>>(),
                    observers
                );
            }
            readonly_pass(&mut forged);
            for _ in 0..2 {
                forged = restored(&forged);
                readonly_pass(&mut forged);
            }
        }
    }
}

#[test]
fn simultaneous_observer_and_opposing_creature_entry_uses_pre_entry_static_and_post_entry_trigger()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for observer_first in [false, true] {
            for existing_observer in [false, true] {
                let mut state = ready(actor);
                if existing_observer {
                    cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
                    settle(&mut state);
                }
                let authority = put(&mut state, actor, "Authority of the Consuls", Zone::Hand);
                let entrant = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
                let observer_entry = ProposedEvent::zone_change(authority, Zone::Battlefield);
                let creature_entry = ProposedEvent::zone_change(entrant, Zone::Battlefield);
                event::propose_and_commit_batch(
                    &mut state,
                    if observer_first {
                        vec![observer_entry, creature_entry]
                    } else {
                        vec![creature_entry, observer_entry]
                    },
                );
                assert_eq!(state.objects.get(entrant).tapped, existing_observer);
                let captures = state
                    .engine
                    .event_log
                    .iter()
                    .filter(|event| {
                        matches!(event,
                    CommittedEvent::AuthorityEntryCaptureV1 { capture }
                    if capture.entrant.object == entrant)
                    })
                    .count();
                assert_eq!(captures, if existing_observer { 2 } else { 1 });
                let mut replay = restored(&state);
                for current in [&mut state, &mut replay] {
                    settle(current);
                    assert_eq!(
                        current.players[actor.index()].life,
                        if existing_observer { 22 } else { 21 }
                    );
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}

#[test]
fn collected_dispatch_deletion_and_resolved_item_replay_refuse_readonly_after_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
        settle(&mut state);
        put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        until_gain(&mut state);
        let saved_item = state.stack.last().unwrap().clone();
        let capture_index = match saved_item.inline_effect {
            Some(EffectOp::AuthorityEntryGainLifeV1 {
                capture_history_index,
            }) => capture_history_index,
            _ => panic!("actual bound gain required"),
        };
        let capture_event =
            state.engine.event_history[usize::try_from(capture_index).unwrap()].clone();
        for corruption in 0..6 {
            let mut forged = restored(&state);
            match corruption {
                0 => {
                    forged.stack.pop();
                    // A matching old stack ID does not prove this operation
                    // resolved or was countered. No operation was committed.
                    forged
                        .engine
                        .event_history
                        .push(CommittedEvent::AuthorityEntryCompletedV1 {
                            capture_history_index: capture_index,
                            stack_item_id: saved_item.v4.stack_item_id,
                            operation_history_index: 0,
                        });
                }
                1 => forged.engine.event_log.push(capture_event.clone()),
                2 => {
                    let mut extra = capture_event.clone();
                    let CommittedEvent::AuthorityEntryCaptureV1 { capture } = &mut extra else {
                        unreachable!()
                    };
                    capture.source.owner = actor.opponent();
                    forged.engine.event_log.push(extra);
                }
                3 => forged.stack.last_mut().unwrap().v4.stack_item_id = Default::default(),
                4 => forged.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::AuthorityEntryPublishedV1 { .. })
                }),
                _ => {
                    let publication = forged
                        .engine
                        .event_history
                        .iter_mut()
                        .find(|event| {
                            matches!(event, CommittedEvent::AuthorityEntryPublishedV1 { .. })
                        })
                        .unwrap();
                    let CommittedEvent::AuthorityEntryPublishedV1 { item, .. } = publication else {
                        unreachable!()
                    };
                    item.controller = actor.opponent();
                }
            }
            readonly_pass(&mut forged);
            let mut repeated = restored(&forged);
            readonly_pass(&mut repeated);
        }
        let mut omitted = restored(&state);
        omitted.stack.pop();
        readonly_pass(&mut omitted);
        let mut repeated = restored(&omitted);
        readonly_pass(&mut repeated);
        settle(&mut state);
        assert_eq!(state.players[actor.index()].life, 21);
        assert_eq!(
            state
                .engine
                .event_history
                .iter()
                .filter(|event| matches!(event, CommittedEvent::AuthorityEntryCompletedV1 { .. }))
                .count(),
            1
        );
        for corruption in 0..5 {
            let mut forged = restored(&state);
            match corruption {
                0 => forged.stack.push(saved_item.clone()),
                1 => forged.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::AuthorityEntryCompletedV1 { .. })
                }),
                2 => {
                    let closed = forged
                        .engine
                        .event_history
                        .iter()
                        .find(|event| {
                            matches!(event, CommittedEvent::AuthorityEntryCompletedV1 { .. })
                        })
                        .unwrap()
                        .clone();
                    forged.engine.event_history.push(closed);
                }
                3 => forged.engine.event_history.retain(|event| {
                    !matches!(event, CommittedEvent::AuthorityEntryOperationV1 { .. })
                }),
                _ => {
                    let operation = forged
                        .engine
                        .event_history
                        .iter_mut()
                        .find(|event| {
                            matches!(event, CommittedEvent::AuthorityEntryOperationV1 { .. })
                        })
                        .unwrap();
                    let CommittedEvent::AuthorityEntryOperationV1 { stack_item_id, .. } = operation
                    else {
                        unreachable!()
                    };
                    *stack_item_id = Default::default();
                }
            }
            readonly_pass(&mut forged);
            for _ in 0..2 {
                forged = restored(&forged);
                readonly_pass(&mut forged);
            }
        }
    }
}

#[test]
fn genuine_legend_suspension_retains_both_entry_gain_owners_and_original_beneficiary() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let authority = cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
        settle(&mut state);
        let first = put(
            &mut state,
            actor.opponent(),
            "Dwynen, Gilt-Leaf Daen",
            Zone::Hand,
        );
        let second = put(
            &mut state,
            actor.opponent(),
            "Dwynen, Gilt-Leaf Daen",
            Zone::Hand,
        );
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(first, Zone::Battlefield),
                ProposedEvent::zone_change(second, Zone::Battlefield),
            ],
        );
        let Decision::ChooseLegendPermanent { player, candidates } = next(&mut state) else {
            panic!("actual duplicate legends must suspend the checkpoint");
        };
        assert_eq!(player, actor.opponent());
        assert_eq!(candidates.len(), 2);
        assert!(state.engine.pending_triggers.is_empty());
        let serialized = serde_json::to_value(&state).unwrap();
        assert_eq!(
            serialized["pending_legend_rule_v1"]["waiting_triggers"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        transfer(&mut state, authority, actor.opponent());
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(authority, Zone::Hand),
        );
        let mut erased_value = serde_json::to_value(&state).unwrap();
        erased_value["pending_legend_rule_v1"]["waiting_triggers"] = serde_json::json!([]);
        let mut erased: GameState = serde_json::from_value(erased_value).unwrap();
        let before = serde_json::to_vec(&erased).unwrap();
        assert!(engine::step(&mut erased, Action::ChooseLegendPermanent(first)).is_err());
        assert_eq!(serde_json::to_vec(&erased).unwrap(), before);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseLegendPermanent(first)).unwrap();
            settle(current);
            assert_eq!(current.players[actor.index()].life, 22);
            assert_eq!(current.players[actor.opponent().index()].life, 20);
            assert_eq!(current.objects.get(second).zone, Zone::Graveyard);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn actual_two_entry_batch_retains_distinct_occurrences_and_restored_trigger_order() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let authority = cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
        settle(&mut state);
        let first = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
        let second = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(first, Zone::Battlefield),
                ProposedEvent::zone_change(second, Zone::Battlefield),
            ],
        );
        assert!(state.objects.get(first).tapped && state.objects.get(second).tapped);
        let captures: Vec<_> = state
            .engine
            .event_log
            .iter()
            .filter_map(|event| match event {
                CommittedEvent::AuthorityEntryCaptureV1 { capture } => Some(capture),
                _ => None,
            })
            .collect();
        assert_eq!(captures.len(), 2);
        assert_ne!(
            captures[0].entry_history_index,
            captures[1].entry_history_index
        );
        assert_ne!(captures[0].entrant.object, captures[1].entrant.object);
        let decision = next(&mut state);
        assert!(
            matches!(decision, Decision::OrderTriggers { player, ref pending } if player == actor && pending.len() == 2)
        );
        transfer(&mut state, authority, actor.opponent());
        transfer(&mut state, first, actor);
        let mut omitted = restored(&state);
        omitted.engine.pending_triggers.remove(0);
        readonly_pass(&mut omitted);
        let mut repeated_omission = restored(&omitted);
        readonly_pass(&mut repeated_omission);
        let mut forged = restored(&state);
        let pending = forged.engine.pending_triggers.first_mut().unwrap();
        let EffectOp::AuthorityEntryGainLifeV1 {
            capture_history_index,
        } = pending.effect
        else {
            panic!("actual captured pending trigger required");
        };
        pending.controller = actor.opponent();
        pending.source_contract.as_mut().unwrap().controller = actor.opponent();
        let CommittedEvent::AuthorityEntryCaptureV1 { capture } =
            &mut forged.engine.event_history[usize::try_from(capture_history_index).unwrap()]
        else {
            panic!("actual committed capture required");
        };
        // Coherently rewrite the pending payer/source and its dispatch capture.
        // Leave the independent entry-observer record unchanged.
        capture.source.controller = actor.opponent();
        capture.entrant_controller = actor;
        readonly_pass(&mut forged);
        let mut repeated = restored(&forged);
        readonly_pass(&mut repeated);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::OrderTriggers(vec![1, 0])).unwrap();
            settle(current);
            assert_eq!(current.players[actor.index()].life, 22);
            assert_eq!(current.players[actor.opponent().index()].life, 20);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
