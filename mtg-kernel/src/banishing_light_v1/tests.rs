//! Actual future definition and accepted physical copy/attachment boundaries.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
use crate::card_def::card_id_by_name;
use crate::engine::{self, Action, Decision};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::state::{GameObject, ObjectStateV4, Step, Target};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("genuine future registration required");
    let object = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
        zone,
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
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        _ => panic!("physical fixture supports hand/battlefield"),
    }
    object
}

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 12],
        &[forest; 12],
        |id| CARD_DEFS[id as usize].name.into(),
        11026,
        actor,
    );
    state.step = Step::Main1;
    state
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn finish_stack(state: &mut GameState) {
    for _ in 0..40 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected boundary {other:?}"),
        }
    }
    panic!("stack did not complete");
}

fn replay(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn genuine_copied_token_source_self_exile_never_returns_before_sba_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        // Physical entry covers the original printed ETB without inventing
        // a copy spell. Actual printed cast/payment is in the integration suite.
        let original = put(&mut state, actor, "Banishing Light", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(original, Zone::Battlefield),
        );
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(
            state.engine.pending_triggers.is_empty(),
            "no opponent nonland target"
        );
        // Genuine accepted allocator: CreateToken and the exact copiable
        // Banishing definition, with the allocator's physical token flag.
        let token =
            crate::standard_cards_v1::create_token_copy(&mut state, original, actor).unwrap();
        state.players[actor.index()]
            .battlefield
            .retain(|object| *object != token);
        state.players[actor.opponent().index()]
            .battlefield
            .push(token);
        state.objects.get_mut(token).controller = actor.opponent();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { player, legal_targets, .. }
            if player == actor && legal_targets.contains(&Target::Object(token)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(token))).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            finish_stack(branch);
            assert!(!branch
                .players
                .iter()
                .any(|player| player.battlefield.contains(&token)));
            assert!(
                !branch.exile.contains(&token),
                "real SBA ceases the exiled token"
            );
            assert!(!branch.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::ZoneChange { object, from: Zone::Exile, to: Zone::Battlefield, .. } if *object == token)));
            assert!(branch.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::BanishingLinkExpiredV1 { record, no_return_reason: Some(NoReturnV1::TokenCannotReturn), .. }
                    if record.exiled == token)));
            validate_state(branch).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
#[ignore = "requires future actual Banishing Light registration; no admission bypass"]
fn exiled_aura_without_host_stays_exiled_and_its_capture_cannot_be_erased_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let creature = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let aura = put(&mut state, actor.opponent(), "Pacifism", Zone::Battlefield);
        state.attach_object_exact(aura, 0, creature, 0).unwrap();
        let source = put(&mut state, actor, "Banishing Light", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(aura)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(aura))).unwrap();
        finish_stack(&mut state);
        assert_eq!(state.objects.get(aura).zone, Zone::Exile);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Graveyard),
        );
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            assert_eq!(branch.objects.get(aura).zone, Zone::Exile);
            assert!(branch.exile.contains(&aura));
            assert!(branch.engine.linked_exile_records.is_empty());
            assert!(!branch.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::ZoneChange { object, from: Zone::Exile, to: Zone::Battlefield, .. } if *object == aura)));
            validate_state(branch).unwrap();
            let mut forged = replay(branch);
            forged.engine.event_history.retain(|event| {
                !matches!(event, CommittedEvent::BanishingAuraHostsCapturedV1 { .. })
            });
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(engine::step(&mut forged, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&forged).unwrap());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
