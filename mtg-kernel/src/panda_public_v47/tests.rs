use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state_for};

#[test]
fn public_absence_is_readonly_hidden_independent_and_forgery_refuses_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut left = ready_state_for(actor);
        let mut right = left.clone();
        put(&mut left, actor.opponent(), "Island", Zone::Hand);
        put(&mut right, actor.opponent(), "Forest", Zone::Hand);
        put(&mut left, actor.opponent(), "Forest", Zone::Library);
        put(&mut right, actor.opponent(), "Island", Zone::Library);
        let before = serde_json::to_vec(&left).unwrap();
        let facts = capture(&left).unwrap();
        assert_eq!(facts, capture(&right).unwrap());
        validate(&left, &facts).unwrap();
        assert!(facts.occurrences.is_empty());
        assert!(facts.trigger_order.is_empty());
        let mut bad = facts.clone();
        bad.card_db_hash ^= 1;
        assert!(validate(&left, &bad).is_err());
        let mut bad = facts.clone();
        bad.producer_sha256.push('0');
        assert!(validate(&left, &bad).is_err());
        let mut unknown = serde_json::to_value(&facts).unwrap();
        unknown["private_world"] = serde_json::json!([]);
        assert!(serde_json::from_value::<FactsV47>(unknown).is_err());
        assert_eq!(serde_json::to_vec(&left).unwrap(), before);
    }
}

#[test]
fn forged_public_capture_cannot_relabel_a_genuine_predecessor_gain() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let gain = state.engine.event_history.len();
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::life_gain(actor, 2),
        );
        let captured = AbilitySourceContractV4::capture(&state, source);
        state
            .engine
            .event_history
            .push(CommittedEvent::FiendishPandaLifeGainCaptureV1 {
                source: captured,
                gain_history_index: gain as u64,
                amount: 2,
            });
        let before = serde_json::to_vec(&state).unwrap();
        assert!(capture(&state).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod printed {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    fn paid_panda(state: &mut GameState, actor: PlayerId) -> crate::ids::ObjectId {
        let source = put(state, actor, "Fiendish Panda", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        engine::step(state, Action::CastSpell(source)).unwrap();
        resolve(state);
        source
    }
    fn resolve(state: &mut GameState) {
        for _ in 0..48 {
            if state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.event_log.is_empty()
            {
                return;
            }
            match engine::advance_until_decision(state) {
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("actual public Panda priority: {other:?}"),
            }
        }
        panic!("Panda actual resolution did not finish");
    }
    #[test]
    #[ignore = "Requires genuine Panda printing and current native qualification"]
    fn genuine_paid_gain_phases_exact_counter_restore_hidden_swap_and_owner_forgery() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = paid_panda(&mut state, actor);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::life_gain(actor, 6),
            );
            let fresh = capture(&state).unwrap();
            assert_eq!(fresh.occurrences.len(), 1);
            assert_eq!(fresh.occurrences[0].phase, PhaseV47::Fresh);
            assert_eq!(fresh.occurrences[0].life_gained, Some(6));
            assert_eq!(fresh.occurrences[0].actual_counters_added, 0);
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            let published = capture(&state).unwrap();
            assert_eq!(published.occurrences[0].phase, PhaseV47::Published);
            let original = state.stack.last().unwrap().clone();
            assert_eq!(
                stack_source_reference(&state, &original).unwrap().unwrap(),
                published.occurrences[0].source
            );
            let mut forged = original;
            forged.mode_chosen = 1;
            assert!(stack_source_reference(&state, &forged).is_err());
            resolve(&mut state);
            let facts = capture(&state).unwrap();
            assert_eq!(facts.occurrences[0].phase, PhaseV47::Resolved);
            assert!(facts.occurrences[0].operation_attempted);
            assert_eq!(facts.occurrences[0].actual_counters_added, 1);
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(capture(&restored).unwrap(), facts);
            let mut hidden = restored.clone();
            put(&mut hidden, actor.opponent(), "Forest", Zone::Hand);
            put(&mut hidden, actor.opponent(), "Island", Zone::Library);
            assert_eq!(capture(&hidden).unwrap(), facts);
            let mut bad = facts.clone();
            bad.occurrences[0].actual_counters_added = 6;
            assert!(validate(&restored, &bad).is_err());
            let mut bad = facts;
            bad.occurrences[0].source.zone_change_count += 1;
            assert!(validate(&restored, &bad).is_err());
        }
    }
    #[test]
    #[ignore = "Requires genuine Panda printing and current native qualification"]
    fn genuine_death_original_target_and_signed_power_survive_return_and_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = paid_panda(&mut state, actor);
            let target = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            let original_generation = state.objects.get(target).zone_change_count;
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            let fresh = capture(&state).unwrap();
            assert_eq!(fresh.occurrences[0].captured_power, Some(3));
            assert!(fresh.occurrences[0].current_original_source.is_none());
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            let pending = state
                .engine
                .pending_triggers
                .iter()
                .find(|p| p.source == source)
                .unwrap()
                .clone();
            let held = capture(&state).unwrap();
            assert_eq!(held.occurrences[0].phase, PhaseV47::Held);
            assert_eq!(
                pending_source_reference(&state, &pending).unwrap().unwrap(),
                held.occurrences[0].source
            );
            let mut forged = pending;
            forged.kicked = true;
            assert!(pending_source_reference(&state, &forged).is_err());
            engine::step(
                &mut state,
                Action::ChooseTarget(crate::state::Target::Object(target)),
            )
            .unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            let published = capture(&state).unwrap();
            let original_target = published.occurrences[0].original_target.as_ref().unwrap();
            assert_eq!(
                original_target_reference(&state, state.stack.last().unwrap())
                    .unwrap()
                    .as_ref(),
                Some(original_target)
            );
            assert_eq!(original_target.arena_id, target.0);
            assert_eq!(original_target.zone, Zone::Graveyard);
            assert_eq!(original_target.zone_change_count, original_generation);
            resolve(&mut state);
            let facts = capture(&state).unwrap();
            let row = &facts.occurrences[0];
            assert_eq!(row.phase, PhaseV47::Resolved);
            assert_eq!(
                row.original_target,
                published.occurrences[0].original_target
            );
            assert_eq!(row.return_destination, Some(Zone::Battlefield));
            assert_eq!(
                row.returned_incarnation,
                Some(IncarnationV47 {
                    object: target.0,
                    generation: original_generation + 1
                })
            );
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(capture(&restored).unwrap(), facts);
            let mut bad = facts;
            bad.occurrences[0].captured_power = Some(4);
            assert!(validate(&restored, &bad).is_err());
        }
    }
    #[test]
    #[ignore = "Requires genuine Panda printing and current native qualification"]
    fn original_public_graveyard_target_remains_bound_after_hidden_departure_and_fizzle() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = paid_panda(&mut state, actor);
            let target = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            engine::step(
                &mut state,
                Action::ChooseTarget(crate::state::Target::Object(target)),
            )
            .unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            let item = state.stack.last().unwrap().clone();
            let original = original_target_reference(&state, &item).unwrap().unwrap();
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(target, Zone::Hand),
            );
            assert_eq!(
                original_target_reference(&state, &item).unwrap(),
                Some(original.clone())
            );
            let published = capture(&state).unwrap();
            assert_eq!(
                published.occurrences[0].original_target,
                Some(original.clone())
            );
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(
                original_target_reference(&restored, &item).unwrap(),
                Some(original.clone())
            );
            resolve(&mut state);
            let facts = capture(&state).unwrap();
            assert_eq!(facts.occurrences[0].phase, PhaseV47::Fizzled);
            assert_eq!(facts.occurrences[0].original_target, Some(original));
            assert!(!facts.occurrences[0].operation_attempted);
            assert!(facts.occurrences[0].returned_incarnation.is_none());
            assert_eq!(state.objects.get(target).zone, Zone::Hand);
        }
    }
}
