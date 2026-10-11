//! Registered typed operands; these do not pretend to carry Wildwood's root.
use super::*;
use crate::card_def::{card_id_by_name, CARD_DEFS};
use crate::effect::{EffectObjectBinding, ExecCtx};
use crate::event::{self, ProposedEvent};

fn ready(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let elf = card_id_by_name("Llanowar Elves").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[elf; 40],
        &[elf; 40],
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_291,
        actor,
    );
    let source = state.players[actor.index()].library[0];
    let recipient = state.players[actor.index()].library[1];
    for id in [source, recipient] {
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(id, Zone::Battlefield),
        );
    }
    (state, source, recipient)
}
fn counter_event(
    state: &GameState,
    object: ObjectId,
    placing: PlayerId,
    count: i32,
) -> CommittedEvent {
    CommittedEvent::PlusOneCountersAdded {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
        player: placing,
        count,
    }
}
#[test]
fn recipient_control_type_and_incarnation_determine_match_not_placing_player() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = ready(actor);
        for placing in [actor, actor.opponent()] {
            for count in [1, 2, 7] {
                let event = counter_event(&state, recipient, placing, count);
                assert!(matches_counter_event(&state, source, actor, &event));
            }
        }
        assert!(!matches_counter_event(
            &state,
            source,
            actor,
            &counter_event(&state, source, actor, 1)
        ));
        assert!(!matches_counter_event(
            &state,
            source,
            actor,
            &counter_event(&state, recipient, actor, 0)
        ));
        let earlier = counter_event(&state, recipient, actor, 1);
        state.objects.get_mut(recipient).controller = actor.opponent();
        assert!(!matches_counter_event(&state, source, actor, &earlier));
        state.objects.get_mut(recipient).controller = actor;
        let hydra = card_id_by_name("Nyxborn Hydra").unwrap();
        // A typed matcher operand; no invented printed copy history or stack root.
        state.objects.get_mut(recipient).card_def = hydra;
        assert!(!matches_counter_event(&state, source, actor, &earlier));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(recipient, Zone::Graveyard),
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(recipient, Zone::Battlefield),
        );
        assert!(!matches_counter_event(&state, source, actor, &earlier));
    }
}
#[test]
fn existing_bound_source_counter_survives_control_and_skips_departed_or_reentered_source() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let (mut state, source, _) = ready(actor);
            let mut ctx = ExecCtx::no_targets(source, actor);
            ctx.ability_source_contract = Some(crate::state::AbilitySourceContractV4::capture(
                &state, source,
            ));
            let op = EffectOp::PutPlusOnePlusOneCounterOnBoundObject {
                object: EffectObjectBinding {
                    object: source,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: state.objects.get(source).zone_change_count,
                },
            };
            if mode == 0 {
                state.objects.get_mut(source).controller = actor.opponent();
            } else {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                if mode == 2 {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
            }
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut replay] {
                crate::effect::execute(&op, &ctx, branch);
                assert_eq!(
                    branch.objects.get(source).counters.plus1_plus1,
                    if mode == 0 { 1 } else { 0 }
                );
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn absent_wildwood_source_leaves_existing_counter_event_and_pending_transport_unchanged() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = ready(actor);
        let event = counter_event(&state, recipient, actor.opponent(), 2);
        let bytes = serde_json::to_vec(&state).unwrap();
        capture_counter_event(&mut state, &event);
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        assert!(state.engine.pending_triggers.is_empty());
        assert_ne!(source, recipient);
    }
}
#[test]
fn real_registered_nyxborn_normal_x_entry_and_restore_both_seats() {
    use crate::engine::{self, Action, Decision};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, _) = ready(actor);
        state.step = crate::state::Step::Main1;
        let hydra = card_id_by_name("Nyxborn Hydra").unwrap();
        let source = state.players[actor.index()].library[0];
        // Library recipe operand, before any captured source/copy authority.
        let old = state.objects.get_mut(source);
        old.card_def = hydra;
        old.name = CARD_DEFS[hydra as usize].object_name.into();
        old.v4 = crate::state::ObjectStateV4::from_card_def(hydra);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::G.pool_index()] = 2;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 3;
        engine::advance_until_decision(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseSpellMode { .. }
        ));
        engine::step(&mut state, Action::ChooseSpellMode(0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseEffectOption { .. }
        ));
        engine::step(&mut state, Action::ChooseEffectOption(3)).unwrap();
        engine::advance_until_decision(&mut state); // Actual finalization.
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            for _ in 0..20 {
                let decision = engine::advance_until_decision(branch);
                if branch.stack.is_empty() {
                    break;
                }
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                engine::step(branch, Action::Pass).unwrap();
            }
            assert_eq!(branch.objects.get(source).zone, Zone::Battlefield);
            assert_eq!(branch.objects.get(source).counters.plus1_plus1, 3);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn absent_unpublished_capture_queue_preserves_legacy_bytes_and_hash() {
    use std::hash::{Hash, Hasher};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, _) = ready(actor);
        assert!(state.wildwood_waiting_v1.is_none());
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(!String::from_utf8(bytes.clone())
            .unwrap()
            .contains("wildwood_waiting_v1"));
        let mut before = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut before);
        assert!(take_captures(&mut state).is_empty());
        let mut after = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut after);
        assert_eq!(before.finish(), after.finish());
        assert_eq!(bytes, serde_json::to_vec(&state).unwrap());
    }
}
