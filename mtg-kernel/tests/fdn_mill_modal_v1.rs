//! Prepared gameplay cases for the next serial FDN admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready() -> GameState {
    let plains = card_id_by_name("Plains").unwrap();
    let mut state =
        GameState::new_from_libraries(&[plains; 12], &[plains; 12], |_| "Plains".into(), 670);
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
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Exile => state.players[player.index()].exile.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..100 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => return Some(other),
        }
    }
    panic!("did not settle")
}

fn cast(state: &mut GameState, name: &str) -> ObjectId {
    let source = put(state, PlayerId::P0, name, Zone::Hand);
    state.players[0].mana_pool = [10; 6];
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    source
}

fn stats(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

#[test]
fn printed_definitions_and_new_subtype_are_complete() {
    let shriek = &CARD_DEFS[card_id_by_name("Billowing Shriekmass").unwrap() as usize];
    let stomper = &CARD_DEFS[card_id_by_name("Apothecary Stomper").unwrap() as usize];
    assert_eq!(
        (
            card_id_by_name("Billowing Shriekmass"),
            card_id_by_name("Apothecary Stomper")
        ),
        (Some(340), Some(341))
    );
    assert_eq!(
        (shriek.power, shriek.toughness, shriek.mana_value),
        (Some(2), Some(3), 4)
    );
    assert!(shriek.keywords.has(Keywords::FLYING));
    assert_eq!(shriek.subtypes, &[Subtype::Spirit]);
    assert_eq!(
        (stomper.power, stomper.toughness, stomper.mana_value),
        (Some(4), Some(4), 6)
    );
    assert!(stomper.keywords.has(Keywords::VIGILANCE));
    assert_eq!(stomper.subtypes, &[Subtype::Elephant]);
}

#[test]
fn threshold_recomputes_for_controller_and_zone_changes() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready();
        let source = put(
            &mut state,
            player,
            "Billowing Shriekmass",
            Zone::Battlefield,
        );
        for _ in 0..6 {
            put(&mut state, player, "Plains", Zone::Graveyard);
        }
        assert_eq!(stats(&state, source), (2, 3));
        let seventh = put(&mut state, player, "Plains", Zone::Graveyard);
        assert_eq!(stats(&state, source), (4, 4));
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(seventh, Zone::Exile));
        assert_eq!(stats(&state, source), (2, 3));
        let other = if player == PlayerId::P0 {
            PlayerId::P1
        } else {
            PlayerId::P0
        };
        for _ in 0..7 {
            put(&mut state, other, "Plains", Zone::Graveyard);
        }
        assert_eq!(stats(&state, source), (2, 3));
        state.players[player.index()]
            .battlefield
            .retain(|&id| id != source);
        state.players[other.index()].battlefield.push(source);
        state.objects.get_mut(source).controller = other;
        assert_eq!(stats(&state, source), (4, 4));
    }
}

#[test]
fn threshold_ignores_transient_tokens_and_only_operates_on_battlefield() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Billowing Shriekmass",
        Zone::Battlefield,
    );
    for _ in 0..6 {
        put(&mut state, PlayerId::P0, "Plains", Zone::Graveyard);
    }
    put(&mut state, PlayerId::P0, "Food Token", Zone::Graveyard);
    assert_eq!(stats(&state, source), (2, 3));
    put(&mut state, PlayerId::P0, "Plains", Zone::Graveyard);
    assert_eq!(stats(&state, source), (4, 4));
    for zone in [Zone::Hand, Zone::Library, Zone::Graveyard, Zone::Exile] {
        let outside = put(&mut state, PlayerId::P0, "Billowing Shriekmass", zone);
        assert_eq!(stats(&state, outside), (2, 3), "{zone:?}");
    }
}

#[test]
fn ability_removal_turns_off_threshold_then_departure_restores_it() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Billowing Shriekmass",
        Zone::Battlefield,
    );
    for _ in 0..7 {
        put(&mut state, PlayerId::P0, "Plains", Zone::Graveyard);
    }
    let aura = cast(&mut state, "Witness Protection");
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
    assert!(settle(&mut state).is_none());
    assert_eq!(stats(&state, source), (1, 1));
    assert!(!engine::has_effective_keyword(
        &state,
        source,
        Keywords::FLYING
    ));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aura, Zone::Graveyard),
    );
    assert_eq!(stats(&state, source), (4, 4));
}

#[test]
fn mill_three_pauses_privately_and_restores_exactly() {
    let mut state = ready();
    for _ in 0..4 {
        put(&mut state, PlayerId::P0, "Plains", Zone::Graveyard);
    }
    let prefix = state.players[0].library[..3].to_vec();
    let source = cast(&mut state, "Billowing Shriekmass");
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            ..
        })
    ));
    assert_eq!(state.players[0].graveyard.len(), 4);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    for game in [&mut state, &mut restored] {
        for &object in &prefix[..2] {
            engine::step(game, Action::ChooseEffectTarget(Target::Object(object))).unwrap();
        }
        assert!(settle(game).is_none());
        assert_eq!(game.players[0].graveyard.len(), 7);
        assert_eq!(stats(game, source), (4, 4));
        assert_eq!(&game.players[0].graveyard[4..], &prefix);
    }
    assert_eq!(state.state_hash(), restored.state_hash());
}

#[test]
fn short_empty_library_mill_does_not_cause_draw_loss() {
    for count in [0, 1] {
        let mut state = ready();
        // Move the unused library tail to exile to keep zone membership valid.
        let removed = state.players[0].library[count..].to_vec();
        for object in removed {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(object, Zone::Exile));
        }
        cast(&mut state, "Billowing Shriekmass");
        assert!(settle(&mut state).is_none());
        assert_eq!(state.players[0].graveyard.len(), count);
        assert!(state.players[0].library.is_empty());
        assert!(!state.players[0].drew_from_empty);
        assert!(!state.players[0].has_lost);
    }
}

#[test]
fn mode_is_chosen_before_targets_and_pending_mode_roundtrips() {
    for mode in [0, 1] {
        let mut state = ready();
        let source = cast(&mut state, "Apothecary Stomper");
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseTriggerMode { .. })
        ));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for game in [&mut state, &mut restored] {
            engine::step(game, Action::ChooseTriggerMode(mode)).unwrap();
            if mode == 0 {
                assert!(matches!(next(game), Decision::ChooseTargets { .. }));
                engine::step(game, Action::ChooseTarget(Target::Object(source))).unwrap();
            }
            assert!(settle(game).is_none());
            assert_eq!(
                game.objects.get(source).counters.plus1_plus1,
                if mode == 0 { 2 } else { 0 }
            );
            assert_eq!(game.players[0].life, if mode == 1 { 24 } else { 20 });
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn counter_mode_rejects_opponent_and_stale_target() {
    let mut state = ready();
    let own = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let other = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    cast(&mut state, "Apothecary Stomper");
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTriggerMode { .. })
    ));
    engine::step(&mut state, Action::ChooseTriggerMode(0)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(other))).is_err());
    engine::step(&mut state, Action::ChooseTarget(Target::Object(own))).unwrap();
    // Put the selected trigger onto the stack before the target blinks.
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(own, Zone::Exile));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(own, Zone::Battlefield),
    );
    assert!(settle(&mut state).is_none());
    assert_eq!(state.objects.get(own).counters.plus1_plus1, 0);
}
