//! Requires genuine future registration. Physical characteristic boundaries
//! below are labeled and do not claim a printed FDN modifier producer.
use super::*;
use crate::engine::{self, Action, Decision};
use crate::ids::PlayerId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{put, ready_state};

fn ready(actor: PlayerId) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    state.step = crate::state::Step::Main1;
    state
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
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected genuine draw stage {other:?}"),
        }
    }
    panic!("draw resolution did not settle");
}
fn cast(state: &mut GameState, actor: PlayerId, name: &str, green: u8, generic: u8) -> ObjectId {
    let source = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = green;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = generic;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    source
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn readonly(state: &GameState) {
    for _ in 0..2 {
        let mut bad = restore(state);
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
    }
}

#[test]
fn actual_cast_conditional_etb_and_later_event_time_draw_trample_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let hand = state.players[actor.index()].hand.len();
        let source = cast(&mut state, actor, "Garruk's Uprising", 1, 2);
        assert_eq!(state.players[actor.index()].hand.len(), hand);
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(elf, Zone::Battlefield),
        );
        settle(&mut state);
        assert_eq!(state.players[actor.index()].hand.len(), hand);
        assert!(engine::has_effective_keyword(
            &state,
            elf,
            Keywords::TRAMPLE
        ));
        let before = state.players[actor.index()].hand.len();
        cast(&mut state, actor, "Myr Enforcer", 0, 7);
        assert_eq!(state.players[actor.index()].hand.len(), before + 1);
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        assert!(!engine::has_effective_keyword(
            &state,
            elf,
            Keywords::TRAMPLE
        ));
        let other = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(other, Zone::Battlefield),
        );
        assert!(engine::has_effective_keyword(
            &state,
            other,
            Keywords::TRAMPLE
        ));
    }
}

#[test]
fn own_etb_rechecks_ferocious_but_entrant_capture_survives_departure_and_control() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let large = cast(&mut state, actor, "Myr Enforcer", 0, 7);
        let source = put(&mut state, actor, "Garruk's Uprising", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 3;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..20 {
            next(&mut state);
            if state.objects.get(source).zone == Zone::Battlefield {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        let before = state.players[actor.index()].hand.len();
        crate::event::propose_and_commit(&mut state, ProposedEvent::zone_change(large, Zone::Hand));
        settle(&mut state);
        assert_eq!(
            state.players[actor.index()].hand.len(),
            before + 1,
            "returned creature only, no conditional draw"
        );
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(large, Zone::Battlefield),
        );
        next(&mut state);
        let before = state.players[actor.index()].hand.len();
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(large, Zone::Graveyard),
        );
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.players[actor.index()].hand.len(), before + 1);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn simultaneous_anthems_are_all_applied_before_every_entrant_power_observation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reversed in [false, true] {
            let mut state = ready(actor);
            cast(&mut state, actor, "Garruk's Uprising", 1, 2);
            let mut birds = (0..3)
                .map(|_| put(&mut state, actor, "Empyrean Eagle", Zone::Hand))
                .collect::<Vec<_>>();
            if reversed {
                birds.reverse();
            }
            crate::event::propose_and_commit_batch(
                &mut state,
                birds
                    .iter()
                    .map(|id| ProposedEvent::zone_change(*id, Zone::Battlefield))
                    .collect(),
            );
            assert!(birds
                .iter()
                .all(|id| engine::effective_power(&state, *id) == 4));
            let before = state.players[actor.index()].hand.len();
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(state.players[actor.index()].hand.len(), before + 3);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn genuine_pending_and_published_draw_ownership_cannot_be_erased_or_replayed() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        cast(&mut state, actor, "Myr Enforcer", 0, 7);
        let source = put(&mut state, actor, "Garruk's Uprising", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 3;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..20 {
            next(&mut state);
            if state.stack.iter().any(|row| {
                matches!(
                    row.inline_effect,
                    Some(EffectOp::GarrukDrawFromCaptureV1 { .. })
                )
            }) {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        let saved = state
            .stack
            .iter()
            .find(|row| {
                matches!(
                    row.inline_effect,
                    Some(EffectOp::GarrukDrawFromCaptureV1 { .. })
                )
            })
            .unwrap()
            .clone();
        let mut bad = restore(&state);
        bad.stack
            .retain(|row| row.v4.stack_item_id != saved.v4.stack_item_id);
        readonly(&bad);
        let mut bad = restore(&state);
        bad.stack.last_mut().unwrap().controller = actor.opponent();
        readonly(&bad);
        settle(&mut state);
        let mut bad = restore(&state);
        bad.stack.push(saved);
        readonly(&bad);
    }
}
