use super::*;

fn order_group(state: crate::state::GameState, actor: PlayerId) -> HumanHunterSessionV45 {
    HumanHunterSessionV45::from_session(
        FastActorSessionV1::from_hunter_v45_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
fn drive_order(mut session: HumanHunterSessionV45, actor: PlayerId, count: usize, hunter: bool) {
    let original = session.inner.hunter_fixture_state().clone();
    let mut complete_order = Vec::new();
    let mut projector = HumanHunterProjectorV45::new(actor.into());
    for position in 0..count {
        let e = expected(&session);
        assert_eq!(
            (e.substep_index, e.substep_count),
            (position as u32, count as u32)
        );
        assert_eq!(e.physical_decision_id, 0);
        let menu = projector.project_current(&session, e).unwrap();
        let (_, raw_actions, _) = session
            .inner
            .hunter_current_decision_input_v45(e, actor.into())
            .unwrap();
        assert_eq!(menu.actions.len(), count - position);
        assert!(menu
            .actions
            .iter()
            .all(|row| row.label.contains("Place ")
                && row.label.contains("last placed resolves first")));
        assert!(menu
            .actions
            .windows(2)
            .all(|rows| rows[0].label != rows[1].label));
        if hunter && position == 0 {
            assert_eq!(
                menu.actions
                    .iter()
                    .filter(|row| row.label.contains("draw one"))
                    .count(),
                9
            );
            assert!(menu
                .state
                .extensions
                .hunter
                .occurrences
                .iter()
                .all(|row| row.current_original_source.is_none()));
        }
        let saved = session.snapshot_v45();
        let first = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: 0,
        };
        // The canonical action is mapped privately to its actual group position.
        let selected = projector.pending.as_ref().unwrap().engine_indexes[0] as usize;
        let action = &raw_actions[selected];
        let crate::rl::ActionSemanticV1::ChooseTriggerOrderNext { trigger_index, .. } = action
        else {
            panic!("{action:?}")
        };
        let chosen = *trigger_index;
        complete_order.push(chosen);
        let receipt = projector.submit(&mut session, first.clone()).unwrap();
        let after = session.inner.hunter_fixture_state().clone();
        assert_eq!(
            projector.submit(&mut session, first.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.hunter_fixture_state(), &after);
        session.restore_v45(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut session, first),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.hunter_fixture_state(), &original);
        let e = expected(&session);
        let menu = projector.project_current(&session, e).unwrap();
        let (_, raw_actions, _) = session
            .inner
            .hunter_current_decision_input_v45(e, actor.into())
            .unwrap();
        let pending = projector.pending.as_ref().unwrap();
        let canonical=pending.engine_indexes.iter().position(|index|matches!(
            raw_actions[*index as usize],
            crate::rl::ActionSemanticV1::ChooseTriggerOrderNext{trigger_index,..}if trigger_index==chosen
        )).unwrap();
        projector
            .submit(
                &mut session,
                HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: canonical as u32,
                },
            )
            .unwrap();
        if position + 1 < count {
            assert_eq!(session.inner.hunter_fixture_state(), &original);
        }
    }
    let state = session.inner.hunter_fixture_state();
    assert_eq!(state.stack.len(), count);
    // One final complete engine permutation, with every original occurrence.
    for (item, index) in state.stack.iter().zip(complete_order) {
        let pending = &original.engine.pending_triggers[index];
        assert_eq!(item.source, pending.source);
        assert_eq!(item.v4.ability_source_contract, pending.source_contract);
        assert_eq!(item.inline_effect.as_ref(), Some(&pending.effect));
    }
    assert!(state.engine.pending_triggers.is_empty());
    assert_eq!(expected(&session).physical_decision_id, 1);
    if hunter {
        for _ in 0..80 {
            if session.inner.hunter_fixture_state().stack.is_empty() {
                break;
            }
            pass(&mut session, &mut projector, actor);
        }
        let facts = session.public_observation(expected(&session)).unwrap();
        assert_eq!(
            facts
                .hunter
                .occurrences
                .iter()
                .filter(|row| row.draw_attempted)
                .count(),
            9
        );
        assert!(session.inner.hunter_fixture_state().stack.is_empty());
    }
}
#[test]
fn registered_nine_distinct_trigger_labels_retry_restore_and_final_aggregate_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = crate::rl_session::hunter_v45::tests::ordering::registered_group(actor);
        drive_order(order_group(state, actor), actor, 9, false);
    }
}
#[test]
#[ignore = "unregistered Hunter; parent owns actual registration/native"]
fn real_simultaneous_hunter_death_nine_draws_plus_ordinary_trigger_complete_mixed_human_order_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let hunter = put(&mut state, actor, "High-Society Hunter", Zone::Battlefield);
        put(&mut state, actor, "Homunculus Horde", Zone::Battlefield);
        state.players[actor.index()].draws_this_turn = 0;
        for _ in 0..2 {
            crate::event::propose_and_commit(&mut state, crate::event::ProposedEvent::draw(actor));
        }
        let mut batch = vec![crate::event::ProposedEvent::zone_change(
            hunter,
            Zone::Graveyard,
        )];
        for index in 0..9 {
            let dead = put(
                &mut state,
                if index % 2 == 0 {
                    actor
                } else {
                    actor.opponent()
                },
                "Llanowar Elves",
                Zone::Battlefield,
            );
            batch.push(crate::event::ProposedEvent::zone_change(
                dead,
                Zone::Graveyard,
            ));
        }
        crate::event::propose_and_commit_batch(&mut state, batch);
        let waiting = crate::trigger::collect_and_process(&mut state);
        assert_eq!(waiting.len(), 10);
        state.engine.pending_triggers.extend(waiting);
        drive_order(order_group(state, actor), actor, 10, true);
    }
}
