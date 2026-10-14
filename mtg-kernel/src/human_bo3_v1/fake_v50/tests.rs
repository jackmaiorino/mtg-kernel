use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::state::Zone;

fn expected(session: &HumanFakeSessionV50) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(value) => value,
        other => panic!("actual Human V50 decision: {other:?}"),
    }
}
fn fixture(actor: PlayerId, hidden: &str) -> HumanFakeSessionV50 {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
    put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    for _ in 0..2 {
        put(&mut state, actor, "Swamp", Zone::Battlefield);
    }
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    put(&mut state, actor.opponent(), "Mountain", Zone::Battlefield);
    HumanFakeSessionV50::from_session(
        FastActorSessionV1::from_fake_v50_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
fn menu(
    session: &HumanFakeSessionV50,
    projector: &mut HumanFakeProjectorV50,
) -> HumanDecisionFakeV50 {
    projector
        .project_current(session, expected(session))
        .unwrap()
}
fn submit(
    session: &mut HumanFakeSessionV50,
    projector: &mut HumanFakeProjectorV50,
    choose: impl Fn(&str) -> bool,
) -> HumanActionRequestV1 {
    let current = menu(session, projector);
    let row = current
        .actions
        .iter()
        .find(|row| choose(&row.label))
        .unwrap_or_else(|| panic!("actual Human V50 action absent: {:?}", current.actions));
    let request = HumanActionRequestV1 {
        prompt_seq: current.prompt_seq,
        action_index: row.action_index,
    };
    projector.submit(session, request.clone()).unwrap();
    request
}
fn drain(
    session: &mut HumanFakeSessionV50,
    projector: &mut HumanFakeProjectorV50,
) -> HumanDecisionFakeV50 {
    for _ in 0..32 {
        let e = expected(session);
        if e.acting_player != session.human_seat {
            session
                .step_opponent_with(e, |_, actions| {
                    actions
                        .iter()
                        .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                        .and_then(|index| u32::try_from(index).ok())
                        .ok_or_else(|| {
                            crate::rl::RlContractError("actual opponent pass absent".into())
                        })
                })
                .unwrap();
        } else if session.inner.fake_fixture_state().stack.is_empty()
            && session
                .inner
                .fake_fixture_state()
                .engine
                .pending_triggers
                .is_empty()
        {
            return menu(session, projector);
        } else {
            submit(session, projector, |label| label == "Pass priority");
        }
    }
    panic!("actual Human V50 spell resolution did not finish");
}
fn gameplay(session: &HumanFakeSessionV50) -> (Vec<u8>, u64, u64) {
    (
        serde_json::to_vec(session.inner.fake_fixture_state()).unwrap(),
        session.inner.policy_step_count(),
        session.inner.physical_decision_count(),
    )
}
#[test]
fn genuine_paid_fake_human_projection_restore_retry_and_foreign_owner_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = fixture(actor, "Lightning Bolt");
        let mut projector = HumanFakeProjectorV50::new(actor.into());
        for _ in 0..2 {
            submit(&mut session, &mut projector, |label| {
                label.contains("Swamp") && label.contains("{B}")
            });
        }
        assert_eq!(
            session.inner.fake_fixture_state().players[actor.index()].mana_pool
                [crate::mana::ManaColor::B.pool_index()],
            2
        );
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Begin casting Fake Your Own Death")
        });
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Fake Your Own Death [")
                && label.contains(": target Llanowar Elves [")
        });
        assert_eq!(
            session.inner.fake_fixture_state().players[actor.index()].mana_pool
                [crate::mana::ManaColor::B.pool_index()],
            0
        );
        let snapshot = session.snapshot_v50();
        let finished = drain(&mut session, &mut projector);
        assert_eq!(finished.state.extensions.fake.spells.len(), 1);
        assert_eq!(finished.state.extensions.fake.grants.len(), 1);
        assert!(finished.state.extensions.fake.grants[0].death_ability_active);
        let (_, target) = session
            .inner
            .fake_fixture_state()
            .objects
            .iter()
            .find(|(_, object)| {
                crate::card_def::CARD_DEFS[usize::from(object.card_def)].name == "Llanowar Elves"
            })
            .unwrap();
        assert_eq!(target.zone, Zone::Battlefield);
        let (target_id, _) = session
            .inner
            .fake_fixture_state()
            .objects
            .iter()
            .find(|(_, object)| {
                crate::card_def::CARD_DEFS[usize::from(object.card_def)].name == "Llanowar Elves"
            })
            .unwrap();
        assert_eq!(
            crate::engine::effective_power(session.inner.fake_fixture_state(), target_id),
            3
        );
        let (_, spell) = session
            .inner
            .fake_fixture_state()
            .objects
            .iter()
            .find(|(_, object)| {
                crate::card_def::CARD_DEFS[usize::from(object.card_def)].name
                    == "Fake Your Own Death"
            })
            .unwrap();
        assert_eq!(spell.zone, Zone::Graveyard);
        assert_eq!(
            finished.state.extensions.fake.spells[0].phase,
            crate::fake_your_own_death_v50::public_v50::SpellPhaseV50::Resolved
        );
        let first = gameplay(&session);
        session.restore_v50(&snapshot).unwrap();
        let replay = drain(&mut session, &mut projector);
        assert_eq!(replay.state, finished.state);
        assert_eq!(replay.actions, finished.actions);
        assert_eq!(gameplay(&session), first);
        let before = session.inner.privileged_core_environment_hash();
        let request = HumanActionRequestV1 {
            prompt_seq: replay.prompt_seq,
            action_index: replay.actions.len() as u32,
        };
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::InvalidAction)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let play = submit(&mut session, &mut projector, |label| {
            label.starts_with("Play") && label.contains("Forest")
        });
        let after = gameplay(&session);
        let receipt = projector.submit(&mut session, play.clone()).unwrap();
        assert_eq!(receipt.action_index, play.action_index);
        assert_eq!(gameplay(&session), after);
        session.restore_v50(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, play),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let foreign =
            HumanFakeSessionV50::from_session(session.inner.clone(), actor.into()).unwrap();
        assert_eq!(
            session.restore_v50(&foreign.snapshot_v50()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}
#[test]
fn actual_fake_human_unknown_hand_invariance_safe_dto_wrong_seat_and_binding_refusal() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut left = fixture(actor, "Lightning Bolt");
        let right = fixture(actor, "Mountain");
        let mut p = HumanFakeProjectorV50::new(actor.into());
        let mut q = HumanFakeProjectorV50::new(actor.into());
        let a = menu(&left, &mut p);
        let b = menu(&right, &mut q);
        assert_eq!(a.state, b.state);
        assert_eq!(a.actions, b.actions);
        let json = serde_json::to_string(&a).unwrap();
        for key in [
            "arena_id",
            "card_db_id",
            "zone_change_count",
            "producer_sha256",
            "history_index",
            "journal_index",
        ] {
            assert!(
                !json.contains(&format!("\"{key}\"")),
                "private field {key}: {json}"
            );
        }
        assert!(!json.contains("Lightning Bolt"));
        let before = left.inner.privileged_core_environment_hash();
        assert_eq!(
            HumanFakeProjectorV50::new(actor.opponent().into())
                .project_current(&left, expected(&left)),
            Err(Error::NotHumanTurn)
        );
        assert_eq!(left.inner.privileged_core_environment_hash(), before);
        let index = a
            .actions
            .iter()
            .find(|row| row.label.starts_with("Play") && row.label.contains("Forest"))
            .unwrap()
            .action_index;
        p.pending.as_mut().unwrap().public.fake.spells.clear();
        p.pending.as_mut().unwrap().public.fake.card_db_hash ^= 1;
        assert_eq!(
            p.submit(
                &mut left,
                HumanActionRequestV1 {
                    prompt_seq: a.prompt_seq,
                    action_index: index
                }
            ),
            Err(Error::StaleDecision)
        );
        assert_eq!(left.inner.privileged_core_environment_hash(), before);
    }
}

#[test]
fn actual_paid_fake_human_masks_unknown_face_down_recipient_before_and_after_grant() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let original = fixture(actor, "Lightning Bolt");
        let mut state = original.inner.fake_fixture_state().clone();
        let (target, _) = state
            .objects
            .iter()
            .find(|(_, object)| {
                crate::card_def::CARD_DEFS[usize::from(object.card_def)].name == "Llanowar Elves"
            })
            .unwrap();
        state.objects.get_mut(target).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        let mut session = HumanFakeSessionV50::from_session(
            FastActorSessionV1::from_fake_v50_fixture_state(state),
            actor.into(),
        )
        .unwrap();
        let mut projector = HumanFakeProjectorV50::new(actor.into());
        for _ in 0..2 {
            submit(&mut session, &mut projector, |label| {
                label.contains("Swamp") && label.contains("{B}")
            });
        }
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Begin casting Fake Your Own Death")
        });
        let targets = menu(&session, &mut projector);
        let json = serde_json::to_string(&targets).unwrap();
        assert!(
            !json.contains("Llanowar Elves"),
            "unknown recipient leaked: {json}"
        );
        assert!(json.contains("Face-down creature"));
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Fake Your Own Death [")
                && label.contains(": target Face-down creature [")
        });
        let finished = drain(&mut session, &mut projector);
        assert_eq!(finished.state.extensions.fake.grants.len(), 1);
        assert_eq!(
            finished.state.extensions.fake.grants[0]
                .original_recipient
                .name,
            "Face-down creature"
        );
        assert!(finished.state.extensions.fake.grants[0].death_ability_active);
        assert_eq!(
            crate::engine::effective_power(session.inner.fake_fixture_state(), target),
            4
        );
        let json = serde_json::to_string(&finished).unwrap();
        assert!(
            !json.contains("Llanowar Elves"),
            "unknown granted recipient leaked: {json}"
        );
    }
}
