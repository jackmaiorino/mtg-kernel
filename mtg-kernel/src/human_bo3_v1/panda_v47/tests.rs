use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::rl_session::panda_v47::tests as backend;
use crate::state::Zone;

fn expected(session: &HumanPandaSessionV47) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(e) = response else {
        panic!("{response:?}")
    };
    e
}
fn session(state: crate::state::GameState, actor: PlayerId) -> HumanPandaSessionV47 {
    HumanPandaSessionV47::from_session(
        FastActorSessionV1::from_panda_v47_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
fn submit(
    session: &mut HumanPandaSessionV47,
    projector: &mut HumanPandaProjectorV47,
    want: impl Fn(&str) -> bool,
) {
    let e = expected(session);
    let menu = projector.project_current(session, e).unwrap();
    let row = menu
        .actions
        .iter()
        .find(|row| want(&row.label))
        .unwrap_or_else(|| panic!("{menu:?}"));
    projector
        .submit(
            session,
            HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: row.action_index,
            },
        )
        .unwrap();
}
#[test]
fn genuine_fixed_seat_hidden_swap_retry_owning_rollback_and_recommitted_public_refusal_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut left = backend::ordinary(actor);
        let mut right = left.clone();
        put(&mut left, actor.opponent(), "Island", Zone::Hand);
        put(&mut right, actor.opponent(), "Forest", Zone::Hand);
        put(&mut left, actor.opponent(), "Forest", Zone::Library);
        put(&mut right, actor.opponent(), "Island", Zone::Library);
        let mut left = session(left, actor);
        let right = session(right, actor);
        let mut projector = HumanPandaProjectorV47::new(actor.into());
        let mut other = HumanPandaProjectorV47::new(actor.into());
        let e = expected(&left);
        let menu = projector.project_current(&left, e).unwrap();
        let other_menu = other.project_current(&right, expected(&right)).unwrap();
        assert_eq!(menu.state, other_menu.state);
        assert_eq!(menu.actions, other_menu.actions);
        assert!(menu.state.extensions.panda.occurrences.is_empty());
        let saved = left.snapshot_v47();
        let row = menu
            .actions
            .iter()
            .find(|row| row.label.starts_with("Play") && row.label.contains("Forest"))
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: row.action_index,
        };
        let receipt = projector.submit(&mut left, request.clone()).unwrap();
        let after = left.inner.panda_fixture_state().clone();
        assert_eq!(
            projector.submit(&mut left, request.clone()).unwrap(),
            receipt
        );
        assert_eq!(left.inner.panda_fixture_state(), &after);
        left.restore_v47(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut left, request),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            left.restore_v47(&right.snapshot_v47()),
            Err(Error::StaleDecision)
        );
        let e = expected(&left);
        let menu = projector.project_current(&left, e).unwrap();
        projector
            .pending
            .as_mut()
            .unwrap()
            .public
            .panda
            .card_db_hash ^= 1;
        let before = left.inner.panda_fixture_state().clone();
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
        assert_eq!(left.inner.panda_fixture_state(), &before);
        assert!(left
            .public_observation(FastActorDecisionV1 {
                acting_player: actor.opponent().into(),
                ..e
            })
            .is_err());
    }
}
#[test]
fn genuine_registered_large_group_keeps_distinct_safe_labels_and_restored_final_aggregate_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = session(backend::nine_group(actor), actor);
        let original = session.inner.panda_fixture_state().clone();
        let mut projector = HumanPandaProjectorV47::new(actor.into());
        let mut chosen = Vec::new();
        for index in 0..9 {
            let e = expected(&session);
            assert_eq!((e.substep_index, e.substep_count), (index, 9));
            let menu = projector.project_current(&session, e).unwrap();
            assert_eq!(menu.actions.len(), 9 - index as usize);
            assert!(menu
                .actions
                .windows(2)
                .all(|rows| rows[0].label != rows[1].label));
            assert!(menu
                .actions
                .iter()
                .all(|row| row.label.contains("last placed resolves first")));
            let (_, raw, _) = session
                .inner
                .panda_current_decision_input_v47(e, actor.into())
                .unwrap();
            let mapped = projector.pending.as_ref().unwrap().engine_indexes[0] as usize;
            let ActionSemanticV1::ChooseTriggerOrderNext { trigger_index, .. } = raw[mapped] else {
                panic!("actual staged choice")
            };
            chosen.push(trigger_index);
            let saved = session.snapshot_v47();
            session.restore_v47(&saved).unwrap();
            let menu = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: menu.prompt_seq,
                        action_index: 0,
                    },
                )
                .unwrap();
            if index < 8 {
                assert_eq!(session.inner.panda_fixture_state(), &original);
            }
        }
        for (row, index) in session.inner.panda_fixture_state().stack.iter().zip(chosen) {
            let pending = &original.engine.pending_triggers[index];
            assert_eq!(row.source, pending.source);
            assert_eq!(row.inline_effect.as_ref(), Some(&pending.effect));
        }
    }
}
#[test]
#[ignore = "genuine Panda printing required; parent owns registration/native"]
fn real_paid_panda_mandatory_death_target_keeps_original_roles_restore_and_old_refusals_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = backend::ordinary(actor);
        let source = put(&mut state, actor, "Fiendish Panda", Zone::Hand);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 1;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
        let mut backend = FastActorSessionV1::from_panda_v47_fixture_state(state);
        backend::choose(
            &mut backend,
            |a| matches!(a,ActionSemanticV1::CastSpell{source:s,..}if s.arena_id==source.0),
        );
        backend::drain(&mut backend);
        // Typed committed death setup; actual printed target, custody and return follow.
        let mut state = backend.panda_fixture_state().clone();
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        let mut owned = session(state, actor);
        let e = expected(&owned);
        let public = owned.public_observation(e).unwrap();
        assert_eq!(public.panda.occurrences[0].captured_power, Some(3));
        assert!(public.panda.occurrences[0]
            .current_original_source
            .is_none());
        assert!(owned.inner.clone().into_eaten_v46().is_err());
        let mut projector = HumanPandaProjectorV47::new(actor.into());
        let menu = projector.project_current(&owned, e).unwrap();
        assert!(menu
            .actions
            .iter()
            .any(|row| row.label.contains("non-Bear") && row.label.contains("captured power 3")));
        let saved = owned.snapshot_v47();
        submit(&mut owned, &mut projector, |label| {
            label.contains("Llanowar Elves")
        });
        let after = owned.public_observation(expected(&owned)).unwrap();
        assert_eq!(
            after.panda.occurrences[0]
                .original_target
                .as_ref()
                .unwrap()
                .arena_id,
            target.0
        );
        owned.restore_v47(&saved).unwrap();
        submit(&mut owned, &mut projector, |label| {
            label.contains("Llanowar Elves")
        });
        for _ in 0..40 {
            if owned.inner.panda_fixture_state().stack.is_empty() {
                break;
            }
            let e = expected(&owned);
            if e.acting_player == actor.into() {
                submit(&mut owned, &mut projector, |label| {
                    label.starts_with("Pass")
                });
            } else {
                owned
                    .step_opponent_with(e, |_, actions| {
                        actions
                            .iter()
                            .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                            .map(|index| index as u32)
                            .ok_or_else(|| {
                                crate::rl::RlContractError("genuine opponent Pass absent".into())
                            })
                    })
                    .unwrap();
            }
        }
        assert_eq!(
            owned.inner.panda_fixture_state().objects.get(target).zone,
            Zone::Battlefield
        );
        let menu = projector.project_current(&owned, expected(&owned)).unwrap();
        let returned = &menu.state.extensions.panda.occurrences[0];
        assert!(returned.operation_attempted);
        assert!(returned
            .returned_incarnation
            .as_ref()
            .unwrap()
            .starts_with("panda_returned_incarnation_"));
        assert_eq!(returned.return_destination, Some(Zone::Battlefield));
    }
}
