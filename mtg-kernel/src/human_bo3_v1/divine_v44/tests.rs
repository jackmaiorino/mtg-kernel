use super::*;
use crate::ids::PlayerId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::put;
use crate::state::Zone;

fn expected(session: &HumanDivineSessionV44) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(e) = response else {
        panic!("actual decision: {response:?}")
    };
    e
}
fn build(actor: PlayerId, count: usize) -> HumanDivineSessionV44 {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    put(&mut state, actor, "Divine Resilience", Zone::Hand);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    for _ in 0..count {
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    }
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    HumanDivineSessionV44::from_session(
        FastActorSessionV1::from_divine_v44_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
fn submit(
    session: &mut HumanDivineSessionV44,
    p: &mut HumanDivineProjectorV44,
    want: impl Fn(&str) -> bool,
) -> HumanActionReceiptV1 {
    let e = expected(session);
    let menu = p.project_current(session, e).unwrap();
    let action = menu
        .actions
        .iter()
        .find(|a| want(&a.label))
        .unwrap_or_else(|| panic!("actual safe menu: {menu:?}"));
    p.submit(
        session,
        HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: action.action_index,
        },
    )
    .unwrap()
}
fn pass(session: &mut HumanDivineSessionV44, p: &mut HumanDivineProjectorV44, actor: PlayerId) {
    let e = expected(session);
    if e.acting_player == actor.into() {
        submit(session, p, |s| s.starts_with("Pass"));
    } else {
        assert!(session.public_observation(e).is_err());
        session
            .step_opponent_with(e, |_, actions| {
                actions
                    .iter()
                    .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                    .map(|n| n as u32)
                    .ok_or_else(|| crate::rl::RlContractError("actual opponent Pass absent".into()))
            })
            .unwrap();
    }
}

#[test]
fn actual_normal_one_kicked_zero_two_three_safe_whole_roster_cost_and_restore_retry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (kicked, count) in [(false, 1), (true, 0), (true, 2), (true, 3)] {
            let mut session = build(actor, count);
            let mut p = HumanDivineProjectorV44::new(actor.into());
            submit(&mut session, &mut p, |s| {
                s.starts_with("Begin casting Divine Resilience")
            });
            let e = expected(&session);
            let menu = p.project_current(&session, e).unwrap();
            let public = session.public_observation(e).unwrap();
            assert!(public.divine.pending.is_some());
            let pending = menu.state.extensions.divine.pending.as_ref().unwrap();
            assert_eq!(
                pending.unkicked_quote.as_ref().unwrap().floating_mana_used
                    [ManaColor::W.pool_index()],
                1
            );
            assert_eq!(
                pending.kicked_quote.as_ref().unwrap().floating_mana_used
                    [ManaColor::W.pool_index()],
                2
            );
            let saved = session.snapshot_v44();
            let old_request = HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: menu
                    .actions
                    .iter()
                    .find(|a| {
                        a.label.contains(if kicked {
                            ": pay kicker"
                        } else {
                            ": do not kick"
                        })
                    })
                    .unwrap()
                    .action_index,
            };
            let receipt = p.submit(&mut session, old_request.clone()).unwrap();
            assert_eq!(
                p.submit(&mut session, old_request.clone()).unwrap(),
                receipt
            );
            session.restore_v44(&saved).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                p.submit(&mut session, old_request),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            submit(&mut session, &mut p, |s| {
                s.contains(if kicked {
                    ": pay kicker"
                } else {
                    ": do not kick"
                })
            });
            for selected in 0..count {
                let e = expected(&session);
                let menu = p.project_current(&session, e).unwrap();
                let pending = menu.state.extensions.divine.pending.as_ref().unwrap();
                assert_eq!(pending.selected.len(), selected);
                assert_eq!(pending.legal_choices.len(), count - selected);
                assert_eq!(pending.maximum, if kicked { None } else { Some(1) });
                assert!(menu.actions.iter().any(|a| a.label.contains("finish with")) == kicked);
                let json = serde_json::to_string(&menu).unwrap();
                for secret in [
                    "arena_id",
                    "card_db_hash",
                    "card_db_id",
                    "zone_change_count",
                    "source_contract",
                    "event_history",
                    "root_sha256",
                    "Lightning Bolt",
                ] {
                    assert!(!json.contains(secret), "leaked {secret}");
                }
                submit(&mut session, &mut p, |s| s.contains(": select "));
            }
            if kicked {
                submit(&mut session, &mut p, |s| s.contains("finish with"));
            }
            let source = session
                .inner
                .divine_fixture_state()
                .stack
                .iter()
                .find(|s| {
                    crate::card_def::CARD_DEFS[usize::from(s.v4.source_contract.unwrap().card_def)]
                        .name
                        == "Divine Resilience"
                })
                .unwrap()
                .source;
            let targets = session
                .inner
                .divine_fixture_state()
                .stack
                .iter()
                .find(|s| s.source == source)
                .unwrap()
                .targets
                .clone();
            let snapshot = session.snapshot_v44();
            let e = expected(&session);
            let public = session.public_observation(e).unwrap();
            assert_eq!(public.divine.finalized[0].targets.len(), count);
            for _ in 0..2 {
                session.restore_v44(&snapshot).unwrap();
                let now = expected(&session);
                assert!(now.environment_revision > e.environment_revision);
                assert_eq!(
                    session.public_observation(now).unwrap().divine,
                    public.divine
                );
            }
            for _ in 0..12 {
                if session.inner.divine_fixture_state().stack.is_empty() {
                    break;
                }
                pass(&mut session, &mut p, actor);
            }
            assert!(session.inner.divine_fixture_state().stack.is_empty());
            assert_eq!(
                session
                    .inner
                    .divine_fixture_state()
                    .objects
                    .get(source)
                    .zone,
                Zone::Graveyard
            );
            for target in targets {
                let crate::state::Target::Object(id) = target else {
                    panic!("actual creature target")
                };
                assert!(crate::engine::has_effective_keyword(
                    session.inner.divine_fixture_state(),
                    id,
                    crate::card_def::Keywords::INDESTRUCTIBLE
                ));
            }
        }
    }
}

#[test]
fn ordinary_and_registered_generic_ward_preserve_old_menus_and_all_private_dto_fields() {
    let (mut state, _, _) = crate::policy_observation_v6::tests::ward_multi_targeter_state();
    crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
    let base = FastActorSessionV1::from_human_v8_fixture_state(state);
    let FastActorResponseV1::Decision(e) = base.current_response() else {
        panic!("actual Ward")
    };
    let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
    let old = HumanDecisionProjectorV9::new(e.acting_player)
        .project_current(&old, e)
        .unwrap();
    let new = HumanDivineSessionV44::from_session(base, e.acting_player).unwrap();
    let new = HumanDivineProjectorV44::new(e.acting_player)
        .project_current(&new, e)
        .unwrap();
    assert_eq!(new.actions, old.actions);
    assert_eq!(new.state.public, old.state.public);
    assert_eq!(new.state.extensions.predecessor, old.state.extensions);
    assert_eq!(
        new.state.known_face_down_cards,
        old.state.known_face_down_cards
    );
    assert_eq!(new.state.poison_counters, old.state.poison_counters);
    assert_eq!(new.state.poison_prevention, old.state.poison_prevention);
    assert_eq!(
        new.state.creatures_attacked_this_turn,
        old.state.creatures_attacked_this_turn
    );
    assert_eq!(new.state.ninja_emblems, old.state.ninja_emblems);
    assert_eq!(new.state.restricted_mana, old.state.restricted_mana);
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        let base = FastActorSessionV1::from_limited_common_fixture_state(state);
        let FastActorResponseV1::Decision(e) = base.current_response() else {
            panic!("actual ordinary priority")
        };
        let old =
            crate::human_bo3_v1::HumanAkromaSessionV40::from_session(base.clone(), actor.into())
                .unwrap();
        let old = crate::human_bo3_v1::HumanAkromaProjectorV40::new(actor.into())
            .project_current(&old, e)
            .unwrap();
        let new = HumanDivineSessionV44::from_session(base, actor.into()).unwrap();
        let new = HumanDivineProjectorV44::new(actor.into())
            .project_current(&new, e)
            .unwrap();
        assert_eq!(new.actions, old.actions);
        assert_eq!(new.state.public, old.state.public);
    }
}

#[test]
fn actual_variable_prompt_is_explicitly_refused_by_old_common_and_frozen_labels_readonly_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = build(actor, 3);
        let mut p = HumanDivineProjectorV44::new(actor.into());
        submit(&mut session, &mut p, |s| {
            s.starts_with("Begin casting Divine Resilience")
        });
        submit(&mut session, &mut p, |s| s.contains(": pay kicker"));
        let state = session.inner.divine_fixture_state().clone();
        let before = serde_json::to_vec(&state).unwrap();
        let surface = crate::policy_surface_v5::PolicySurfaceV5::new_with_priority_mode_v1(
            crate::surface_v2::PriorityModeV1::EngineWindowsV1,
            crate::surface_v2::SuppressionAuditMode::Off,
        );
        assert!(
            crate::akroma_public_v1::observe_v1(&state, &surface, actor, 0, 0, (0, 1)).is_err()
        );
        assert!(crate::engine::validate_legacy_variable_cast_targets(&state).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let e = expected(&session);
        let (o, actions, binding) = session
            .inner
            .divine_current_decision_input_v44(e, actor.into())
            .unwrap();
        let (_, _, _, handles) =
            project_base_with_handles(&o, &actions, actor.into(), &binding.families).unwrap();
        assert!(actions.iter().all(|a| super::super::labels::label_parts_v8(
            a,
            &parts(&o),
            &handles,
            actor.into()
        )
        .is_err()));
        let menu = p.project_current(&session, e).unwrap();
        assert!(menu
            .actions
            .iter()
            .any(|a| a.label.contains("finish with 0")));
    }
}

#[test]
fn actual_own_hidden_recipient_uses_safe_mask_and_departed_visibility_not_new_hidden_incarnation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        put(&mut state, actor, "Divine Resilience", Zone::Hand);
        put(&mut state, actor, "Forest", Zone::Hand);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        // Typed face-down operand setup; the cast itself uses the actual printed Divine root.
        state.objects.get_mut(target).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        let mut session = HumanDivineSessionV44::from_session(
            FastActorSessionV1::from_divine_v44_fixture_state(state),
            actor.into(),
        )
        .unwrap();
        let mut p = HumanDivineProjectorV44::new(actor.into());
        submit(&mut session, &mut p, |s| {
            s.starts_with("Begin casting Divine Resilience")
        });
        submit(&mut session, &mut p, |s| s.contains(": pay kicker"));
        let e = expected(&session);
        let menu = p.project_current(&session, e).unwrap();
        let json = serde_json::to_string(&menu).unwrap();
        assert!(!json.contains("Llanowar Elves"));
        assert!(json.contains("Face-down creature"));
        submit(&mut session, &mut p, |s| s.contains(": select "));
        submit(&mut session, &mut p, |s| s.contains("finish with"));
        crate::event::propose_and_commit(
            session.inner.divine_fixture_state_mut(),
            crate::event::ProposedEvent::zone_change(target, Zone::Hand),
        );
        let state = session.inner.divine_fixture_state().clone();
        let item = state
            .stack
            .iter()
            .find(|item| {
                crate::card_def::CARD_DEFS[usize::from(item.v4.source_contract.unwrap().card_def)]
                    .name
                    == "Divine Resilience"
            })
            .unwrap();
        let reference =
            crate::divine_public_v44::target_ref_v44(&state, item.v4.target_contracts[0]).unwrap();
        assert_eq!(
            reference.card_db_id,
            crate::card_def::card_id_by_name("Face-down creature").unwrap()
        );
        let visible = HumanDivineSessionV44::from_session(
            FastActorSessionV1::from_divine_v44_fixture_state(state.clone()),
            actor.into(),
        )
        .unwrap();
        let observation = visible.public_observation(expected(&visible)).unwrap();
        let stack = observation
            .projection
            .surface
            .stack
            .iter()
            .find(|row| row.source.arena_id == item.source.0)
            .unwrap();
        assert_eq!(
            stack.targets,
            vec![crate::rl::TargetRefV1::Object {
                object: reference.clone()
            }]
        );
        let safe = HumanDivineProjectorV44::new(actor.into())
            .project_current(&visible, expected(&visible))
            .unwrap();
        assert_eq!(
            safe.state.extensions.divine.finalized[0].targets[0].name,
            "Face-down creature"
        );
        let mut replay: crate::state::GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            crate::divine_public_v44::target_ref_v44(&replay, item.v4.target_contracts[0]).unwrap(),
            reference
        );
        replay.engine.event_history.retain(|e|!matches!(e,crate::event::CommittedEvent::ProtectionSourceColorDepartureV1{snapshot,..} if snapshot.source.source==target));
        let before = serde_json::to_vec(&replay).unwrap();
        assert!(
            crate::divine_public_v44::target_ref_v44(&replay, item.v4.target_contracts[0]).is_err()
        );
        assert_eq!(serde_json::to_vec(&replay).unwrap(), before);
    }
}

#[test]
fn actual_paid_ajani_entry_and_kicked_divine_share_complete_mixed_facts_and_safe_owning_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let ajani = put(&mut state, actor, "Ajani, Caller of the Pride", Zone::Hand);
        put(&mut state, actor, "Divine Resilience", Zone::Hand);
        put(&mut state, actor, "Forest", Zone::Hand);
        let targets = (0..3)
            .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
            .collect::<Vec<_>>();
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 4;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        let mut session = HumanDivineSessionV44::from_session(
            FastActorSessionV1::from_divine_v44_fixture_state(state),
            actor.into(),
        )
        .unwrap();
        let mut p = HumanDivineProjectorV44::new(actor.into());
        submit(&mut session, &mut p, |s| {
            s.starts_with("Begin casting Ajani")
        });
        for _ in 0..12 {
            if session.inner.divine_fixture_state().stack.is_empty() {
                break;
            }
            pass(&mut session, &mut p, actor);
        }
        assert_eq!(
            session.inner.divine_fixture_state().objects.get(ajani).zone,
            Zone::Battlefield
        );
        let e = expected(&session);
        let o = session.public_observation(e).unwrap();
        assert_eq!(o.ajani.permanents.len(), 1);
        assert_eq!(o.ajani.permanents[0].loyalty, 4);
        submit(&mut session, &mut p, |s| {
            s.starts_with("Begin casting Divine Resilience")
        });
        submit(&mut session, &mut p, |s| s.contains(": pay kicker"));
        for _ in &targets {
            submit(&mut session, &mut p, |s| s.contains(": select "));
        }
        let e = expected(&session);
        let o = session.public_observation(e).unwrap();
        assert_eq!(o.ajani.permanents.len(), 1);
        assert_eq!(o.divine.pending.as_ref().unwrap().selected.len(), 3);
        let save = session.snapshot_v44();
        let menu = p.project_current(&session, e).unwrap();
        assert_eq!(menu.state.extensions.ajani.permanents.len(), 1);
        assert_eq!(
            menu.state
                .extensions
                .divine
                .pending
                .as_ref()
                .unwrap()
                .selected
                .len(),
            3
        );
        session.restore_v44(&save).unwrap();
        let now = expected(&session);
        assert_eq!(session.public_observation(now).unwrap().ajani, o.ajani);
        submit(&mut session, &mut p, |s| s.contains("finish with 3"));
        for _ in 0..12 {
            if session.inner.divine_fixture_state().stack.is_empty() {
                break;
            }
            pass(&mut session, &mut p, actor);
        }
        assert!(session.inner.divine_fixture_state().stack.is_empty());
        for id in targets {
            assert!(crate::engine::has_effective_keyword(
                session.inner.divine_fixture_state(),
                id,
                crate::card_def::Keywords::INDESTRUCTIBLE
            ));
        }
        assert_eq!(
            session
                .public_observation(expected(&session))
                .unwrap()
                .ajani
                .permanents[0]
                .loyalty,
            4
        );
    }
}
