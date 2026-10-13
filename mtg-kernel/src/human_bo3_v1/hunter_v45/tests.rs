use super::*;
use crate::ids::PlayerId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::put;
use crate::state::Zone;
mod linked;
mod simultaneous;

fn expected(session: &HumanHunterSessionV45) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(e) = response else {
        panic!("{response:?}")
    };
    e
}
fn ordinary(actor: PlayerId) -> HumanHunterSessionV45 {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    HumanHunterSessionV45::from_session(
        FastActorSessionV1::from_hunter_v45_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
fn submit(
    session: &mut HumanHunterSessionV45,
    projector: &mut HumanHunterProjectorV45,
    want: impl Fn(&str) -> bool,
) {
    let e = expected(session);
    let menu = projector.project_current(session, e).unwrap();
    let action = menu
        .actions
        .iter()
        .find(|row| want(&row.label))
        .unwrap_or_else(|| panic!("{menu:?}"));
    projector
        .submit(
            session,
            HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: action.action_index,
            },
        )
        .unwrap();
}
fn pass(
    session: &mut HumanHunterSessionV45,
    projector: &mut HumanHunterProjectorV45,
    actor: PlayerId,
) {
    let e = expected(session);
    if e.acting_player == actor.into() {
        submit(session, projector, |label| label.starts_with("Pass"));
    } else {
        assert!(session.public_observation(e).is_err());
        session
            .step_opponent_with(e, |_, actions| {
                actions
                    .iter()
                    .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                    .map(|i| i as u32)
                    .ok_or_else(|| crate::rl::RlContractError("actual opponent Pass absent".into()))
            })
            .unwrap();
    }
}
#[test]
fn registered_actor_privacy_idempotent_retry_fixed_seat_and_same_projector_rollback_refusal_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = ordinary(actor);
        let mut projector = HumanHunterProjectorV45::new(actor.into());
        let e = expected(&session);
        let menu = projector.project_current(&session, e).unwrap();
        assert_eq!(projector.project_current(&session, e).unwrap(), menu);
        let public = session.public_observation(e).unwrap();
        public.validate_artifact().unwrap();
        let json = serde_json::to_string(&menu).unwrap();
        for private in [
            "arena_id",
            "card_db_id",
            "card_db_hash",
            "source_contract",
            "event_history",
            "original_library",
            "Lightning Bolt",
        ] {
            assert!(!json.contains(private), "{private}")
        }
        let snapshot = session.snapshot_v45();
        let action = menu
            .actions
            .iter()
            .find(|row| row.label.contains("Play") && row.label.contains("Forest"))
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: action.action_index,
        };
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        let paid = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), paid);
        session.restore_v45(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let e = expected(&session);
        let refreshed = projector.project_current(&session, e).unwrap();
        assert!(refreshed.prompt_seq > menu.prompt_seq);
        let mut foreign = ordinary(actor);
        assert_eq!(foreign.restore_v45(&snapshot), Err(Error::StaleDecision));
        let mut opposite = HumanHunterProjectorV45::new(actor.opponent().into());
        assert_eq!(
            opposite.project_current(&session, e),
            Err(Error::NotHumanTurn)
        );
    }
}
#[test]
fn registered_public_facts_and_entire_executable_menu_rederive_before_submit_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..4 {
            let mut session = ordinary(actor);
            let mut projector = HumanHunterProjectorV45::new(actor.into());
            let e = expected(&session);
            let menu = projector.project_current(&session, e).unwrap();
            let pending = projector.pending.as_mut().unwrap();
            match case {
                0 => pending.public.own_hand[0].stable.card_db_id ^= 1,
                1 => pending.public.hunter.card_db_hash ^= 1,
                2 => pending.public.hunter.producer_sha256.push('0'),
                _ => pending.expected.legal_action_count += 1,
            }
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: menu.prompt_seq,
                        action_index: 0
                    }
                ),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
        }
    }
}
#[test]
#[ignore = "unregistered Hunter; parent owns actual registration/native"]
fn actual_paid_hunter_optional_menu_and_closed_draw_safe_source_restore_retry_and_predecessor_refusal_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let source = put(&mut state, actor, "High-Society Hunter", Zone::Hand);
            put(&mut state, actor, "Forest", Zone::Hand);
            put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
            let mut session = HumanHunterSessionV45::from_session(
                FastActorSessionV1::from_hunter_v45_fixture_state(state),
                actor.into(),
            )
            .unwrap();
            let mut projector = HumanHunterProjectorV45::new(actor.into());
            submit(&mut session, &mut projector, |label| {
                label.starts_with("Cast High-Society Hunter")
            });
            for _ in 0..20 {
                if session
                    .inner
                    .hunter_fixture_state()
                    .objects
                    .get(source)
                    .zone
                    == Zone::Battlefield
                {
                    break;
                }
                pass(&mut session, &mut projector, actor);
            }
            assert_eq!(
                session
                    .inner
                    .hunter_fixture_state()
                    .objects
                    .get(source)
                    .zone,
                Zone::Battlefield
            );
            let mut state = session.inner.hunter_fixture_state().clone();
            let generation = state.objects.get(source).zone_change_count;
            crate::effect::install_temporary_boost(
                &mut state,
                crate::effect::EffectObjectBinding {
                    object: source,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: generation,
                },
                0,
                0,
                crate::card_def::Keywords::HASTE,
            );
            state.step = crate::state::Step::DeclareAttackers;
            crate::engine::step(
                &mut state,
                crate::engine::Action::DeclareAttackers(vec![source]),
            )
            .unwrap();
            session = HumanHunterSessionV45::from_session(
                FastActorSessionV1::from_hunter_v45_fixture_state(state),
                actor.into(),
            )
            .unwrap();
            projector = HumanHunterProjectorV45::new(actor.into());
            for _ in 0..20 {
                if session
                    .inner
                    .hunter_fixture_state()
                    .engine
                    .pending_effect
                    .as_ref()
                    .is_some_and(|p| p.choice.is_some())
                {
                    break;
                }
                pass(&mut session, &mut projector, actor);
            }
            let e = expected(&session);
            let menu = projector.project_current(&session, e).unwrap();
            let public = session.public_observation(e).unwrap();
            assert!(public.hunter.menu.is_some());
            assert!(menu.state.extensions.hunter.menu.is_some());
            let original = session.inner.privileged_core_environment_hash();
            assert!(crate::human_bo3_v1::HumanDivineSessionV44::from_session(
                session.inner.clone(),
                actor.into()
            )
            .is_err());
            assert_eq!(session.inner.privileged_core_environment_hash(), original);
            let saved = session.snapshot_v45();
            let action = menu
                .actions
                .iter()
                .find(|row| {
                    if pay {
                        row.label.contains(": sacrifice Llanowar Elves")
                    } else {
                        row.label.contains("do not sacrifice")
                    }
                })
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: action.action_index,
            };
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            session.restore_v45(&saved).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            submit(&mut session, &mut projector, |label| {
                if pay {
                    label.contains(": sacrifice Llanowar Elves")
                } else {
                    label.contains("do not sacrifice")
                }
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
                session
                    .inner
                    .hunter_fixture_state()
                    .objects
                    .get(source)
                    .counters
                    .plus1_plus1,
                i32::from(pay)
            );
            let e = expected(&session);
            let after = projector.project_current(&session, e).unwrap();
            assert!(
                after
                    .state
                    .extensions
                    .hunter
                    .occurrences
                    .iter()
                    .all(|row| row.phase
                        == crate::high_society_hunter_public_v45::PhaseV45::Completed)
            );
            assert_eq!(
                after
                    .state
                    .extensions
                    .hunter
                    .occurrences
                    .iter()
                    .filter(|row| row.draw_attempted)
                    .count(),
                usize::from(pay)
            );
            let json = serde_json::to_string(&after).unwrap();
            for private in [
                "arena_id",
                "card_db_id",
                "zone_change_count",
                "capture_history_index",
                "original_library",
                "event_history",
                "Lightning Bolt",
            ] {
                assert!(!json.contains(private), "{private}")
            }
        }
    }
}
