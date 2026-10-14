use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::rl_session::season_v48::tests as backend;
use crate::state::Zone;
fn expected(session: &HumanSeasonSessionV48) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(expected) = response else {
        panic!("{response:?}")
    };
    expected
}
fn session(state: crate::state::GameState, actor: PlayerId) -> HumanSeasonSessionV48 {
    HumanSeasonSessionV48::from_session(
        FastActorSessionV1::from_season_v48_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
#[test]
fn registered_absence_hidden_hand_foreign_owner_retry_restore_and_current_facts_binding() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = backend::ordinary(actor);
        let mut changed = state.clone();
        put(&mut state, actor.opponent(), "Island", Zone::Hand);
        put(&mut changed, actor.opponent(), "Forest", Zone::Hand);
        put(&mut state, actor.opponent(), "Forest", Zone::Library);
        put(&mut changed, actor.opponent(), "Island", Zone::Library);
        let mut left = session(state, actor);
        let right = session(changed, actor);
        let mut projector = HumanSeasonProjectorV48::new(actor.into());
        let mut other = HumanSeasonProjectorV48::new(actor.into());
        let e = expected(&left);
        let menu = projector.project_current(&left, e).unwrap();
        let other_menu = other.project_current(&right, expected(&right)).unwrap();
        assert_eq!(menu.state, other_menu.state);
        assert_eq!(menu.actions, other_menu.actions);
        assert!(menu.state.extensions.season.active_sources.is_empty());
        let saved = left.snapshot_v48();
        let action = menu
            .actions
            .iter()
            .find(|row| row.label.starts_with("Play") && row.label.contains("Forest"))
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: action.action_index,
        };
        let receipt = projector.submit(&mut left, request.clone()).unwrap();
        let after = left.inner.season_fixture_state().clone();
        assert_eq!(
            projector.submit(&mut left, request.clone()).unwrap(),
            receipt
        );
        assert_eq!(left.inner.season_fixture_state(), &after);
        left.restore_v48(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut left, request),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            left.restore_v48(&right.snapshot_v48()),
            Err(Error::StaleDecision)
        );
        let e = expected(&left);
        let menu = projector.project_current(&left, e).unwrap();
        projector
            .pending
            .as_mut()
            .unwrap()
            .public
            .season
            .card_db_hash ^= 1;
        let before = left.inner.season_fixture_state().clone();
        assert_eq!(
            projector.submit(
                &mut left,
                HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: 0
                }
            ),
            Err(Error::StaleDecision)
        );
        assert_eq!(left.inner.season_fixture_state(), &before);
        assert!(left
            .public_observation(FastActorDecisionV1 {
                acting_player: actor.opponent().into(),
                ..e
            })
            .is_err());
    }
}

#[test]
#[ignore = "future actual Doubling Season registration required"]
fn genuine_printed_cast_and_replacement_facts_fixed_seat_owned_restore_both_seats() {
    use crate::event::{self, ProposedEvent};
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = backend::ordinary(actor);
        put(&mut state, actor, "Forest", Zone::Library);
        put(&mut state, actor.opponent(), "Forest", Zone::Battlefield);
        let source = put(&mut state, actor, "Doubling Season", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        let mut session = session(state, actor);
        let mut projector = HumanSeasonProjectorV48::new(actor.into());
        let e = expected(&session);
        let menu = projector.project_current(&session, e).unwrap();
        let cast = menu
            .actions
            .iter()
            .find(|row| row.label.starts_with("Begin casting Doubling Season"))
            .unwrap();
        projector
            .submit(
                &mut session,
                HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: cast.action_index,
                },
            )
            .unwrap();
        assert_eq!(
            session
                .inner
                .season_fixture_state()
                .objects
                .get(source)
                .zone,
            Zone::Stack
        );
        assert_eq!(
            session.inner.season_fixture_state().players[actor.index()].mana_pool
                [ManaColor::G.pool_index()],
            0
        );
        assert_eq!(
            session.inner.season_fixture_state().players[actor.index()].mana_pool
                [ManaColor::C.pool_index()],
            0
        );
        for _ in 0..20 {
            if session
                .inner
                .season_fixture_state()
                .objects
                .get(source)
                .zone
                == Zone::Battlefield
            {
                break;
            }
            let e = expected(&session);
            if e.acting_player == actor.into() {
                let menu = projector.project_current(&session, e).unwrap();
                let pass = menu
                    .actions
                    .iter()
                    .find(|row| row.label == "Pass priority")
                    .unwrap();
                projector
                    .submit(
                        &mut session,
                        HumanActionRequestV1 {
                            prompt_seq: menu.prompt_seq,
                            action_index: pass.action_index,
                        },
                    )
                    .unwrap();
            } else {
                session
                    .step_opponent_with(e, |_, actions| {
                        Ok(actions
                            .iter()
                            .position(|action| {
                                matches!(action, crate::rl::ActionSemanticV1::Pass { .. })
                            })
                            .unwrap() as u32)
                    })
                    .unwrap();
            }
        }
        assert_eq!(
            session
                .inner
                .season_fixture_state()
                .objects
                .get(source)
                .zone,
            Zone::Battlefield
        );
        let saved = session.snapshot_v48();
        session.restore_v48(&saved).unwrap();
        let e = expected(&session);
        let public = session.public_observation(e).unwrap();
        assert_eq!(public.season.active_sources.len(), 1);
        assert_eq!(public.season.meanings.len(), 3);
        let menu = projector.project_current(&session, e).unwrap();
        assert_eq!(menu.state.extensions.season.active_sources.len(), 1);
        // A genuine existing token-creation primitive under the printed static
        // source. This does not invent an additional printed Season ability.
        let mut physical = session.inner.season_fixture_state().clone();
        let token = crate::card_def::card_id_by_name("Goblin Token").unwrap();
        event::propose_and_commit_batch(
            &mut physical,
            vec![ProposedEvent::create_token(token, actor)],
        );
        let mut rebuilt = HumanSeasonSessionV48::from_session(
            FastActorSessionV1::from_season_v48_fixture_state(physical),
            actor.into(),
        )
        .unwrap();
        let e = expected(&rebuilt);
        let public = rebuilt.public_observation(e).unwrap();
        assert_eq!(public.season.creations.len(), 1);
        assert_eq!(public.season.creations[0].created.len(), 2);
        let mut own = HumanSeasonProjectorV48::new(actor.into());
        let menu = own.project_current(&rebuilt, e).unwrap();
        assert_eq!(menu.state.extensions.season.creations[0].created.len(), 2);
        assert!(menu.state.extensions.season.creations[0]
            .created
            .iter()
            .all(|role| role.starts_with("replacement_incarnation_")));
        let saved = rebuilt.snapshot_v48();
        for _ in 0..3 {
            rebuilt.restore_v48(&saved).unwrap();
        }
    }
}

#[test]
#[ignore = "future actual Doubling Season registration required"]
fn genuine_created_token_attachment_human_exact_choice_private_binding_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, equipment) =
            crate::season_token_attachment_v48::tests::actual_primitive_checkpoint(actor);
        let mut session = session(state, actor);
        let mut projector = HumanSeasonProjectorV48::new(actor.into());
        let first = expected(&session);
        let menu = projector.project_current(&session, first).unwrap();
        assert_eq!(menu.schema, "mtg-kernel-human-season-decision/v48");
        assert_eq!(menu.actions.len(), 2);
        assert!(menu
            .actions
            .iter()
            .all(|action| action.label.starts_with("Attach Longbow")));
        let snapshot = session.snapshot_v48();
        for _ in 0..3 {
            session.restore_v48(&snapshot).unwrap();
            let before = session.inner.season_fixture_state().clone();
            assert!(projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: menu.prompt_seq,
                        action_index: menu.actions[0].action_index
                    }
                )
                .is_err());
            assert_eq!(session.inner.season_fixture_state(), &before);
        }
        let e = expected(&session);
        let menu = projector.project_current(&session, e).unwrap();
        projector
            .submit(
                &mut session,
                HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: menu.actions[0].action_index,
                },
            )
            .unwrap();
        let state = session.inner.season_fixture_state();
        assert!(state.objects.get(equipment).v4.attached_to.is_some());
        let facts = crate::doubling_season_public_v48::capture(state).unwrap();
        assert!(facts.attachment_menu.is_none());
        assert_eq!(facts.attachments.len(), 1);
        assert_eq!(facts.creations[0].created.len(), 2);
    }
}
