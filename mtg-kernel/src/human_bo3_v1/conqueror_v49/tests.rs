use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::rl_session::conqueror_v49::tests as backend;
use crate::state::Zone;
fn expected(session: &HumanConquerorSessionV49) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(expected) = response else {
        panic!("{response:?}")
    };
    expected
}
fn session(state: crate::state::GameState, actor: PlayerId) -> HumanConquerorSessionV49 {
    HumanConquerorSessionV49::from_session(
        FastActorSessionV1::from_conqueror_v49_fixture_state(state),
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
        let mut projector = HumanConquerorProjectorV49::new(actor.into());
        let mut other = HumanConquerorProjectorV49::new(actor.into());
        let e = expected(&left);
        let menu = projector.project_current(&left, e).unwrap();
        let other_menu = other.project_current(&right, expected(&right)).unwrap();
        assert_eq!(menu.state, other_menu.state);
        assert_eq!(menu.actions, other_menu.actions);
        assert!(menu.state.extensions.conqueror.occurrences.is_empty());
        let saved = left.snapshot_v49();
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
        let after = left.inner.conqueror_fixture_state().clone();
        assert_eq!(
            projector.submit(&mut left, request.clone()).unwrap(),
            receipt
        );
        assert_eq!(left.inner.conqueror_fixture_state(), &after);
        left.restore_v49(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut left, request),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            left.restore_v49(&right.snapshot_v49()),
            Err(Error::StaleDecision)
        );
        let e = expected(&left);
        let menu = projector.project_current(&left, e).unwrap();
        projector
            .pending
            .as_mut()
            .unwrap()
            .public
            .conqueror
            .card_db_hash ^= 1;
        let before = left.inner.conqueror_fixture_state().clone();
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
        assert_eq!(left.inner.conqueror_fixture_state(), &before);
        assert!(left
            .public_observation(FastActorDecisionV1 {
                acting_player: actor.opponent().into(),
                ..e
            })
            .is_err());
    }
}

fn pass(
    session: &mut HumanConquerorSessionV49,
    projector: &mut HumanConquerorProjectorV49,
    actor: PlayerId,
) {
    let e = expected(session);
    if e.acting_player == actor.into() {
        let menu = projector.project_current(session, e).unwrap();
        let action = menu
            .actions
            .iter()
            .find(|row| row.label == "Pass priority")
            .unwrap();
        projector
            .submit(
                session,
                HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: action.action_index,
                },
            )
            .unwrap();
    } else {
        session
            .step_opponent_with(e, |_, actions| {
                Ok(actions
                    .iter()
                    .position(|action| matches!(action, ActionSemanticV1::Pass { .. }))
                    .unwrap() as u32)
            })
            .unwrap();
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn real_human_paid_printing_saved_batch_gain_pridemate_side_effect_and_owned_replay_both_seats() {
    use crate::event::{self, ProposedEvent};
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = backend::ordinary(actor);
        let source = put(&mut state, actor, "Bloodthirsty Conqueror", Zone::Hand);
        let pridemate = put(&mut state, actor, "Ajani's Pridemate", Zone::Battlefield);
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        let mut current = session(state, actor);
        let mut projector = HumanConquerorProjectorV49::new(actor.into());
        let menu = projector
            .project_current(&current, expected(&current))
            .unwrap();
        assert_eq!(menu.schema, "mtg-kernel-human-conqueror-decision/v49");
        let action = menu
            .actions
            .iter()
            .find(|row| row.label.starts_with("Cast Bloodthirsty Conqueror"))
            .unwrap();
        projector
            .submit(
                &mut current,
                HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: action.action_index,
                },
            )
            .unwrap();
        assert_eq!(
            current
                .inner
                .conqueror_fixture_state()
                .objects
                .get(source)
                .zone,
            Zone::Stack
        );
        assert!(
            current.inner.conqueror_fixture_state().players[actor.index()]
                .mana_pool
                .iter()
                .all(|amount| *amount == 0)
        );
        for _ in 0..20 {
            if current
                .inner
                .conqueror_fixture_state()
                .objects
                .get(source)
                .zone
                == Zone::Battlefield
            {
                break;
            }
            pass(&mut current, &mut projector, actor);
        }
        assert_eq!(
            current
                .inner
                .conqueror_fixture_state()
                .objects
                .get(source)
                .zone,
            Zone::Battlefield
        );
        let mut state = current.inner.conqueror_fixture_state().clone();
        // Genuine simultaneous committed loss/payment operation, followed by the
        // actual printed gain and registered Pridemate trigger.
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::life_loss(actor.opponent(), 2),
                ProposedEvent::life_payment(actor.opponent(), 3),
            ],
        );
        let mut current = session(state, actor);
        let mut projector = HumanConquerorProjectorV49::new(actor.into());
        let public = current.public_observation(expected(&current)).unwrap();
        assert_eq!(public.conqueror.occurrences.len(), 1);
        assert_eq!(public.conqueror.occurrences[0].saved_actual_amount, 5);
        assert_eq!(
            public.conqueror.occurrences[0].original_controller,
            actor.into()
        );
        let saved = current.snapshot_v49();
        let menu = projector
            .project_current(&current, expected(&current))
            .unwrap();
        assert_eq!(
            menu.state.extensions.conqueror.occurrences[0].saved_actual_amount,
            5
        );
        let request = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: menu
                .actions
                .iter()
                .find(|row| row.label == "Pass priority")
                .unwrap()
                .action_index,
        };
        let original = current.inner.conqueror_fixture_state().clone();
        projector
            .pending
            .as_mut()
            .unwrap()
            .public
            .conqueror
            .occurrences[0]
            .saved_actual_amount = 7;
        assert_eq!(
            projector.submit(&mut current, request.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(current.inner.conqueror_fixture_state(), &original);
        current.restore_v49(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut current, request),
            Err(Error::StaleDecision)
        );
        for _ in 0..30 {
            if current.inner.conqueror_fixture_state().stack.is_empty()
                && current
                    .inner
                    .conqueror_fixture_state()
                    .engine
                    .pending_triggers
                    .is_empty()
            {
                break;
            }
            pass(&mut current, &mut projector, actor);
        }
        let finished = current.inner.conqueror_fixture_state();
        assert_eq!(finished.players[actor.index()].life, 25);
        assert_eq!(finished.players[actor.opponent().index()].life, 15);
        assert_eq!(finished.objects.get(pridemate).counters.plus1_plus1, 1);
        assert_eq!(
            crate::conqueror_public_v49::capture(finished)
                .unwrap()
                .occurrences[0]
                .actual_gain,
            Some(5)
        );
        let finished = finished.clone();
        let completed = current.snapshot_v49();
        current.restore_v49(&completed).unwrap();
        assert_eq!(current.inner.conqueror_fixture_state(), &finished);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn real_eight_occurrence_human_stage_safe_labels_private_origin_and_retry_after_restore_both_seats()
{
    use crate::bloodthirsty_conqueror_v49::tests::future_printed;
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = future_printed::paid(actor);
        for _ in 0..8 {
            event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 1));
        }
        let mut current = session(state, actor);
        let mut projector = HumanConquerorProjectorV49::new(actor.into());
        for chosen in 0..8 {
            let e = expected(&current);
            let menu = projector.project_current(&current, e).unwrap();
            let stage = menu.state.extensions.trigger_order.as_ref().unwrap();
            assert_eq!(stage.pending_count, 8);
            assert_eq!(stage.ordered_prefix.len(), chosen);
            assert_eq!(menu.actions.len(), 8 - chosen);
            let labels = menu
                .actions
                .iter()
                .map(|row| row.label.clone())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(labels.len(), menu.actions.len());
            assert!(labels.iter().all(
                |label| label.contains("gain 1 life") && label.contains("Conqueror occurrence")
            ));
            let saved = current.snapshot_v49();
            let before = serde_json::to_vec(current.inner.conqueror_fixture_state()).unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: menu.actions[0].action_index,
            };
            current.restore_v49(&saved).unwrap();
            assert_eq!(
                projector.submit(&mut current, request),
                Err(Error::StaleDecision)
            );
            let menu = projector
                .project_current(&current, expected(&current))
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: menu.actions[0].action_index,
            };
            let receipt = projector.submit(&mut current, request.clone()).unwrap();
            let after = serde_json::to_vec(current.inner.conqueror_fixture_state()).unwrap();
            assert_eq!(projector.submit(&mut current, request).unwrap(), receipt);
            assert_eq!(
                serde_json::to_vec(current.inner.conqueror_fixture_state()).unwrap(),
                after
            );
            if chosen < 7 {
                assert_eq!(after, before);
            }
        }
        assert!(current
            .inner
            .conqueror_fixture_state()
            .engine
            .pending_triggers
            .is_empty());
        assert_eq!(
            current
                .inner
                .conqueror_fixture_state()
                .stack
                .iter()
                .filter(|row| crate::conqueror_public_v49::selected(row))
                .count(),
            8
        );
        crate::bloodthirsty_conqueror_v49::validate(current.inner.conqueror_fixture_state())
            .unwrap();
    }
}
