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
