use super::*;
use crate::event::{self, ProposedEvent};
use crate::policy_observation_v6::tests::{put, ready_state_for};

fn bytes(state: &GameState) -> Vec<u8> {
    serde_json::to_vec(state).unwrap()
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&bytes(state)).unwrap()
}

#[test]
fn genuine_counter_primitive_has_no_new_absent_source_state_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        assert!(!requires_successor(&state));
        let before = bytes(&state);
        let captured = ReplacementInputV48::capture(&state, actor);
        assert!(captured.seasons.is_empty());
        assert_eq!(captured.replaced_count(3).unwrap(), 3);
        assert_eq!(bytes(&state), before);
        for origin in [
            CounterOriginV48::Effect,
            CounterOriginV48::Cost,
            CounterOriginV48::Rule,
        ] {
            assert_eq!(
                counter_amount(&state, creature, actor.opponent(), origin, 3).unwrap(),
                3
            );
        }
        event::add_plus_one_counters(&mut state, creature, actor.opponent(), 3).unwrap();
        assert_eq!(state.objects.get(creature).counters.plus1_plus1, 3);
        assert_eq!(restore(&state), state);
        let before = bytes(&state);
        assert!(event::add_plus_one_counters(&mut state, creature, actor, i32::MAX).is_err());
        assert_eq!(bytes(&state), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn cast_season(state: &mut GameState, actor: PlayerId) -> ObjectId {
    use crate::engine::{self, Action, Decision};
    let source = put(state, actor, "Doubling Season", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(state),Decision::CastSpellOrPass{player,..} if player==actor)
    );
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    for _ in 0..32 {
        if state.objects.get(source).zone == Zone::Battlefield {
            return source;
        }
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual Season spell did not resolve");
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[ignore = "future actual Doubling Season registration required"]
fn actual_printed_source_recipient_controller_multiple_sources_cost_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        for p in [actor, actor.opponent()] {
            put(&mut state, p, "Forest", Zone::Battlefield);
            put(&mut state, p, "Forest", Zone::Library);
        }
        let first = cast_season(&mut state, actor);
        let creature = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        assert_eq!(
            counter_amount(
                &state,
                creature,
                actor.opponent(),
                CounterOriginV48::Effect,
                1
            )
            .unwrap(),
            2
        );
        assert_eq!(
            counter_amount(&state, creature, actor, CounterOriginV48::Cost, 1).unwrap(),
            1
        );
        let mut restored = restore(&state);
        event::add_plus_one_counters(&mut restored, creature, actor.opponent(), 2).unwrap();
        assert_eq!(restored.objects.get(creature).counters.plus1_plus1, 4);
        crate::standard_cards_v1::gain_control(&mut restored, first, actor.opponent());
        assert_eq!(
            counter_amount(&restored, creature, actor, CounterOriginV48::Effect, 1).unwrap(),
            1
        );
        crate::standard_cards_v1::gain_control(&mut restored, creature, actor.opponent());
        assert_eq!(
            counter_amount(&restored, creature, actor, CounterOriginV48::Effect, 1).unwrap(),
            2
        );
        event::propose_and_commit(
            &mut restored,
            ProposedEvent::zone_change(first, Zone::Graveyard),
        );
        assert_eq!(
            counter_amount(&restored, creature, actor, CounterOriginV48::Effect, 1).unwrap(),
            1
        );
        // Actual second printed permanent enters by the accepted effect entry.
        let second = put(&mut state, actor, "Doubling Season", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(second, Zone::Battlefield),
        );
        assert_eq!(sources(&state, actor).len(), 2);
        event::add_plus_one_counters(&mut state, creature, actor.opponent(), 1).unwrap();
        assert_eq!(state.objects.get(creature).counters.plus1_plus1, 4);
        let before = bytes(&state);
        assert!(event::add_plus_one_counters(&mut state, creature, actor, i32::MAX).is_err());
        assert_eq!(bytes(&state), before);
        assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
        assert_eq!(bytes(&state), before);
    }
}
