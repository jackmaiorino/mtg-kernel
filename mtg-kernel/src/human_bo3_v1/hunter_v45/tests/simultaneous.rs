use super::*;

#[test]
#[ignore = "unregistered Hunter; parent owns actual registration/native"]
fn simultaneous_source_death_keeps_two_distinct_human_draw_occurrences_and_original_source_after_restore(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let hunter = put(&mut state, actor, "High-Society Hunter", Zone::Battlefield);
        let one = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let two = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        put(&mut state, actor, "Forest", Zone::Hand);
        // Genuine simultaneous typed death batch, not a manufactured trigger row.
        crate::event::propose_and_commit_batch(
            &mut state,
            vec![
                crate::event::ProposedEvent::zone_change(hunter, Zone::Graveyard),
                crate::event::ProposedEvent::zone_change(one, Zone::Graveyard),
                crate::event::ProposedEvent::zone_change(two, Zone::Graveyard),
            ],
        );
        let before = state.players[actor.index()].hand.len();
        let mut session = HumanHunterSessionV45::from_session(
            FastActorSessionV1::from_hunter_v45_fixture_state(state),
            actor.into(),
        )
        .unwrap();
        let mut projector = HumanHunterProjectorV45::new(actor.into());
        let e = expected(&session);
        let public = session.public_observation(e).unwrap();
        assert_eq!(public.hunter.occurrences.len(), 2);
        assert_eq!(public.hunter.trigger_order.len(), 2);
        assert!(public
            .hunter
            .occurrences
            .iter()
            .all(|row| row.current_original_source.is_none()));
        let menu = projector.project_current(&session, e).unwrap();
        assert_eq!(menu.actions.len(), 2);
        assert_ne!(menu.actions[0].label, menu.actions[1].label);
        assert!(menu
            .actions
            .iter()
            .all(|row| row.label.contains("draw one") && row.label.contains("occurrence")));
        let saved = session.snapshot_v45();
        let request = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: 0,
        };
        projector.submit(&mut session, request.clone()).unwrap();
        session.restore_v45(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        submit(&mut session, &mut projector, |label| {
            label.contains("Put triggers on the stack")
        });
        for _ in 0..30 {
            if session.inner.hunter_fixture_state().stack.is_empty()
                && session
                    .inner
                    .hunter_fixture_state()
                    .engine
                    .pending_triggers
                    .is_empty()
            {
                break;
            }
            pass(&mut session, &mut projector, actor);
        }
        assert_eq!(
            session.inner.hunter_fixture_state().players[actor.index()]
                .hand
                .len(),
            before + 2
        );
        let e = expected(&session);
        let facts = session.public_observation(e).unwrap();
        assert_eq!(
            facts
                .hunter
                .occurrences
                .iter()
                .filter(|row| row.draw_attempted)
                .count(),
            2
        );
        assert!(facts
            .hunter
            .occurrences
            .iter()
            .all(|row| row.source.arena_id == hunter.0 && row.current_original_source.is_none()));
    }
}
