use super::*;

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn request(id: &str) -> LimitedCommonRequestV2 {
    LimitedCommonRequestV2::Current {
        schema_version: SCHEMA_VERSION,
        request_id: id.into(),
    }
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn decision(reply: &LimitedCommonReplyV2) -> (u64, u64, &LimitedCommonDecisionV2) {
    let LimitedCommonBodyV2::State { state, .. } = &reply.body else {
        panic!("state: {reply:?}");
    };
    let LimitedCommonStateV2::Decision {
        episode_id,
        authority_generation,
        decision,
    } = state.as_ref()
    else {
        panic!("decision: {reply:?}");
    };
    (*episode_id, *authority_generation, decision)
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn step(reply: &LimitedCommonReplyV2, id: &str, text: &str) -> LimitedCommonRequestV2 {
    let (episode_id, authority_generation, menu) = decision(reply);
    let action = menu
        .actions
        .iter()
        .find(|action| action.label.starts_with(text))
        .unwrap_or_else(|| {
            panic!(
                "missing {text}; actor={:?}; offered={:?}",
                menu.actor,
                menu.actions
                    .iter()
                    .map(|action| &action.label)
                    .collect::<Vec<_>>()
            )
        });
    LimitedCommonRequestV2::Step {
        schema_version: SCHEMA_VERSION,
        request_id: id.into(),
        episode_id,
        authority_generation,
        actor: menu.actor,
        prompt_seq: menu.prompt_seq,
        action_index: action.action_index,
    }
}
fn deck(name: &str, count: u32) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: vec![crate::limited_session_v1::CustomCardCountV1 {
            name: name.into(),
            count,
        }],
    }
}
fn reset(actor: PlayerSeatV1, id: &str) -> LimitedCommonRequestV2 {
    LimitedCommonRequestV2::Reset {
        schema_version: SCHEMA_VERSION,
        request_id: id.into(),
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 7,
        env_seed: 912,
        starting_player: actor,
        decks: [deck("Forest", 40), deck("Mountain", 41)],
        max_physical_decisions: 10_000,
        max_policy_steps: 10_000,
    }
}
fn error(reply: &LimitedCommonReplyV2, expected: &str) {
    assert_eq!(
        reply.body,
        LimitedCommonBodyV2::Error {
            code: expected.into()
        }
    );
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn drain_fake(core: &mut FastActorSessionV1) {
    for _ in 0..80 {
        let state = core.fake_fixture_state();
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
        {
            return;
        }
        let FastActorResponseV1::Decision(expected) = core.current_response() else {
            panic!("genuine complete V50 priority")
        };
        let (_, actions, binding) = core
            .fake_current_decision_input_v50(expected, expected.acting_player)
            .unwrap();
        let index = actions
            .iter()
            .position(|action| matches!(action, crate::rl::ActionSemanticV1::Pass { .. }))
            .unwrap();
        core.fake_validate_bound_decision_v50(expected, expected.acting_player, &binding)
            .unwrap();
        core.step(
            expected.episode_id,
            expected.step,
            u32::try_from(index).unwrap(),
        )
        .unwrap();
    }
    panic!("genuine complete V50 resolution did not finish");
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn drain_server(
    server: &mut LimitedCommonJsonlServerV2,
    mut reply: LimitedCommonReplyV2,
) -> LimitedCommonReplyV2 {
    for index in 0..80 {
        let state = server.active.as_ref().unwrap().core.fake_fixture_state();
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
        {
            return reply;
        }
        reply = server.handle_request(step(&reply, &format!("resolve{index}"), "Pass priority"));
    }
    panic!("actual V2 resolution did not finish");
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn paid_fake_and_bolt_return_treasure_safe_facts_restore_retry_both_seats() {
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::put;
    use crate::state::Zone;
    for actor in [PlayerId::P0, PlayerId::P1] {
        // Controlled paid lifecycle fixture complements the separate genuine
        // reference-deck public reset, without replacing its shuffled input.
        let mut state = crate::rl_session::panda_v47::tests::ordinary(actor);
        put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        for _ in 0..2 {
            put(&mut state, actor, "Swamp", Zone::Battlefield);
        }
        put(&mut state, actor, "Mountain", Zone::Battlefield);
        let mut server = LimitedCommonJsonlServerV2 {
            active: Some(Active {
                core: FastActorSessionV1::from_fake_v50_fixture_state(state),
                owner: Arc::new(()),
            }),
            generation: 1,
            ..Default::default()
        };
        let mut reply = server.handle_request(request("initial_fake"));
        for index in 0..2 {
            let label = decision(&reply)
                .2
                .actions
                .iter()
                .find(|action| action.label.contains("Swamp") && action.label.contains("{B}"))
                .unwrap()
                .label
                .clone();
            reply = server.handle_request(step(&reply, &format!("black_mana{index}"), &label));
        }
        reply = server.handle_request(step(
            &reply,
            "cast_fake",
            "Begin casting Fake Your Own Death",
        ));
        let label = decision(&reply)
            .2
            .actions
            .iter()
            .find(|action| {
                action.label.starts_with("Fake Your Own Death [")
                    && action.label.contains(": target Llanowar Elves [")
            })
            .unwrap()
            .label
            .clone();
        reply = server.handle_request(step(&reply, "fake_target", &label));
        assert_eq!(
            server
                .active
                .as_ref()
                .unwrap()
                .core
                .fake_fixture_state()
                .players[actor.index()]
            .mana_pool[crate::mana::ManaColor::B.pool_index()],
            0
        );
        let saved = server.handle_request(LimitedCommonRequestV2::Snapshot {
            schema_version: SCHEMA_VERSION,
            request_id: "save_paid_fake".into(),
        });
        let LimitedCommonBodyV2::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = saved.body
        else {
            panic!("paid Fake snapshot")
        };
        let mut first = None;
        for replay in 0..2 {
            if replay == 1 {
                let stale = step(&reply, "stale_after_restore", "Pass priority");
                reply = server.handle_request(LimitedCommonRequestV2::Restore {
                    schema_version: SCHEMA_VERSION,
                    request_id: "restore_fake".into(),
                    snapshot_id,
                });
                let before = server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .privileged_core_environment_hash();
                error(&server.handle_request(stale), "stale_decision");
                assert_eq!(
                    server
                        .active
                        .as_ref()
                        .unwrap()
                        .core
                        .privileged_core_environment_hash(),
                    before
                );
            }
            reply = drain_server(&mut server, reply);
            assert!(decision(&reply).2.state.extensions.fake.grants[0].death_ability_active);
            let label = decision(&reply)
                .2
                .actions
                .iter()
                .find(|action| action.label.contains("Mountain") && action.label.contains("{R}"))
                .unwrap()
                .label
                .clone();
            reply = server.handle_request(step(&reply, "red_mana", &label));
            reply =
                server.handle_request(step(&reply, "cast_bolt", "Begin casting Lightning Bolt"));
            let label = decision(&reply)
                .2
                .actions
                .iter()
                .find(|action| {
                    action.label.starts_with("Target Llanowar Elves [")
                        && action.label.contains(" with Lightning Bolt [")
                })
                .unwrap_or_else(|| panic!("Bolt target labels: {:?}", decision(&reply).2.actions))
                .label
                .clone();
            reply = server.handle_request(step(&reply, "bolt_target", &label));
            reply = drain_server(&mut server, reply);
            let facts = &decision(&reply).2.state.extensions.fake;
            assert_eq!(facts.deaths.len(), 1);
            assert_eq!(facts.deaths[0].treasures.len(), 1);
            let core = &server.active.as_ref().unwrap().core;
            assert_eq!(
                core.fake_fixture_state().objects.get(target).zone,
                Zone::Battlefield
            );
            assert!(core.fake_fixture_state().objects.get(target).tapped);
            let actual = (
                serde_json::to_vec(core.fake_fixture_state()).unwrap(),
                core.policy_step_count(),
                core.physical_decision_count(),
            );
            if let Some(expected) = &first {
                assert_eq!(&actual, expected);
            } else {
                first = Some(actual);
            }
            let json = serde_json::to_string(&reply).unwrap();
            for private in [
                "arena_id",
                "card_db_id",
                "event_history",
                "private_snapshot_json",
                "source_contract",
            ] {
                assert!(!json.contains(private), "{private}");
            }
        }
    }
}

#[test]
fn strict_requests_catalog_mismatch_and_unsupported_decks_do_not_replace_session() {
    let mut server = LimitedCommonJsonlServerV2::new();
    error(
        &server.handle_request(LimitedCommonRequestV2::Current {
            schema_version: 1,
            request_id: "predecessor_schema".into(),
        }),
        "invalid_header",
    );
    let mut predecessor = crate::limited_common_session_v1::LimitedCommonJsonlServerV1::new();
    assert!(predecessor
        .handle_line(
            r#"{"request_type":"current","schema_version":2,"request_id":"successor_schema"}"#
        )
        .contains("invalid_header"));
    let duplicate = server.handle_line(r#"{"request_type":"current","schema_version":2,"schema_version":2,"request_id":"duplicate"}"#);
    assert!(duplicate.contains("invalid_request"));
    let unknown = server.handle_line(
        r#"{"request_type":"current","schema_version":2,"request_id":"unknown","observer":"p1"}"#,
    );
    assert!(unknown.contains("invalid_request"));
    let mut bad = reset(PlayerSeatV1::P0, "catalog");
    if let LimitedCommonRequestV2::Reset {
        expected_card_db_hash,
        ..
    } = &mut bad
    {
        *expected_card_db_hash ^= 1;
    }
    error(&server.handle_request(bad), "catalog_mismatch");
    assert!(server.active.is_none());
    let mut bad = reset(PlayerSeatV1::P0, "missing");
    if let LimitedCommonRequestV2::Reset { decks, .. } = &mut bad {
        decks[0] = deck("A card absent from every kernel catalog", 40);
    }
    error(&server.handle_request(bad), "unsupported_deck");
    assert!(server.active.is_none());
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn fresh_complete_family_decks_select_v50_before_london_and_preserve_real_priority_both_seats() {
    for name in [
        "Ajani, Caller of the Pride",
        "Faebloom Trick",
        "Divine Resilience",
        "High-Society Hunter",
        "Eaten Alive",
        "Fiendish Panda",
    ] {
        for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
            let mut server = LimitedCommonJsonlServerV2::new();
            let mut input = reset(actor, "current_family");
            if let LimitedCommonRequestV2::Reset { decks, .. } = &mut input {
                let own = if actor == PlayerSeatV1::P0 { 0 } else { 1 };
                decks[own] = deck(name, 40);
            }
            let mut reply = server.handle_request(input);
            assert_eq!(decision(&reply).2.actor, actor);
            assert_eq!(decision(&reply).2.state.own_hand.len(), 7);
            assert_eq!(
                decision(&reply).2.schema,
                "mtg-kernel-limited-common-decision/v2"
            );
            let core = &server.active.as_ref().unwrap().core;
            let bound = server.pending.as_ref().unwrap();
            assert!(core
                .fake_current_decision_input_v50(bound.expected, bound.expected.acting_player)
                .is_ok());
            for index in 0..2 {
                reply =
                    server.handle_request(step(&reply, &format!("keep{index}"), "Keep this hand"));
            }
            assert!(decision(&reply)
                .2
                .actions
                .iter()
                .any(|row| row.label == "Pass priority"));
        }
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn real_paid_angel_panda_trigger_safe_menu_owning_restore_and_complete_outcome_both_seats() {
    use crate::ids::PlayerId;
    use crate::mana::ManaColor;
    use crate::policy_observation_v6::tests::put;
    use crate::state::Zone;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::rl_session::panda_v47::tests::ordinary(actor);
        let angel = put(&mut state, actor, "Dazzling Angel", Zone::Hand);
        let panda = put(&mut state, actor, "Fiendish Panda", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        let mut server = LimitedCommonJsonlServerV2 {
            active: Some(Active {
                core: FastActorSessionV1::from_fake_v50_fixture_state(state),
                owner: Arc::new(()),
            }),
            generation: 1,
            ..Default::default()
        };
        let mut reply = server.handle_request(request("initial"));
        reply = server.handle_request(step(&reply, "cast_angel", "Begin casting Dazzling Angel"));
        for index in 0..40 {
            let state = server.active.as_ref().unwrap().core.fake_fixture_state();
            if state.objects.get(angel).zone == Zone::Battlefield
                && state.stack.is_empty()
                && state.engine.pending_cast.is_none()
                && state.engine.pending_triggers.is_empty()
            {
                break;
            }
            reply =
                server.handle_request(step(&reply, &format!("angel_pass{index}"), "Pass priority"));
        }
        assert_eq!(
            server
                .active
                .as_ref()
                .unwrap()
                .core
                .fake_fixture_state()
                .objects
                .get(angel)
                .zone,
            Zone::Battlefield
        );
        assert_eq!(decision(&reply).2.actor, PlayerSeatV1::from(actor));
        let state = server.active.as_ref().unwrap().core.fake_fixture_state();
        assert_eq!(state.step, crate::state::Step::Main1);
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::W.pool_index()],
            1
        );
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
            1
        );
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            2
        );
        reply = server.handle_request(step(&reply, "cast_panda", "Cast Fiendish Panda ["));
        for index in 0..40 {
            if server
                .active
                .as_ref()
                .unwrap()
                .core
                .fake_fixture_state()
                .stack
                .iter()
                .any(crate::panda_public_v47::selected)
            {
                break;
            }
            reply =
                server.handle_request(step(&reply, &format!("panda_pass{index}"), "Pass priority"));
        }
        let facts = &decision(&reply).2.state.extensions.panda.occurrences;
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].life_gained, Some(1));
        assert_eq!(facts[0].phase, crate::panda_public_v47::PhaseV47::Published);
        let json = serde_json::to_string(&reply).unwrap();
        for private in [
            "arena_id",
            "card_db_id",
            "private_snapshot_json",
            "source_contract",
            "event_history",
        ] {
            assert!(!json.contains(private), "{private}");
        }
        let mut uninterrupted = server.active.as_ref().unwrap().core.clone();
        let saved = server.handle_request(LimitedCommonRequestV2::Snapshot {
            schema_version: SCHEMA_VERSION,
            request_id: "save_trigger".into(),
        });
        let LimitedCommonBodyV2::State {
            snapshot_id: Some(id),
            ..
        } = saved.body
        else {
            panic!("actual snapshot")
        };
        let stale = step(&reply, "old_pass", "Pass priority");
        reply = server.handle_request(LimitedCommonRequestV2::Restore {
            schema_version: SCHEMA_VERSION,
            request_id: "restore_trigger".into(),
            snapshot_id: id,
        });
        let before = server
            .active
            .as_ref()
            .unwrap()
            .core
            .privileged_core_environment_hash();
        error(&server.handle_request(stale), "stale_decision");
        assert_eq!(
            server
                .active
                .as_ref()
                .unwrap()
                .core
                .privileged_core_environment_hash(),
            before
        );
        for index in 0..40 {
            let state = server.active.as_ref().unwrap().core.fake_fixture_state();
            if state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
            {
                break;
            }
            reply = server.handle_request(step(&reply, &format!("finish{index}"), "Pass priority"));
        }
        let core = &server.active.as_ref().unwrap().core;
        assert_eq!(
            core.fake_fixture_state()
                .objects
                .get(panda)
                .counters
                .plus1_plus1,
            1
        );
        assert_eq!(
            decision(&reply).2.state.extensions.panda.occurrences[0].actual_counters_added,
            1
        );
        assert!(decision(&reply).2.state.extensions.panda.occurrences[0].operation_attempted);
        drain_fake(&mut uninterrupted);
        assert_eq!(
            serde_json::to_vec(core.fake_fixture_state()).unwrap(),
            serde_json::to_vec(uninterrupted.fake_fixture_state()).unwrap()
        );
        let (FastActorResponseV1::Decision(actual), FastActorResponseV1::Decision(reference)) =
            (core.current_response(), uninterrupted.current_response())
        else {
            panic!("complete paid Panda replay retains genuine priority")
        };
        assert_eq!(
            (actual.step, actual.physical_decision_id),
            (reference.step, reference.physical_decision_id)
        );
        assert!(actual.environment_revision > reference.environment_revision);
    }
}

#[cfg(not(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
)))]
#[test]
fn default_and_standard_reset_refuse_the_new_limited_protocol() {
    let mut server = LimitedCommonJsonlServerV2::new();
    error(
        &server.handle_request(reset(PlayerSeatV1::P0, "wrong_catalog")),
        "unsupported_reset",
    );
    assert!(server.active.is_none());
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn real_london_both_seats_bottom_choice_privacy_and_full_priority() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut server = LimitedCommonJsonlServerV2::new();
        let opening = server.handle_request(reset(actor, "reset"));
        let (_, _, menu) = decision(&opening);
        assert_eq!(menu.actor, actor);
        assert_eq!(menu.state.own_hand.len(), 7);
        assert_eq!(menu.london_mulligans.as_ref().unwrap().counts, [0, 0]);
        let serialized = serde_json::to_string(&opening).unwrap();
        let hidden_name = if actor == PlayerSeatV1::P0 {
            "Mountain"
        } else {
            "Forest"
        };
        assert!(!serialized.contains(hidden_name));
        for private in [
            "arena_id",
            "event_history",
            "private_snapshot_json",
            "engine_indexes",
            "source_contract",
        ] {
            assert!(!serialized.contains(private));
        }
        let mulligan = step(&opening, "mulligan", "Mulligan:");
        let second = server.handle_request(mulligan);
        assert_ne!(decision(&second).2.actor, actor);
        let keep = step(&second, "second_keep", "Keep this hand");
        let bottom = server.handle_request(keep);
        assert_eq!(decision(&bottom).2.actor, actor);
        let choose = step(&bottom, "bottom", "Put ");
        let kept_again = server.handle_request(choose);
        assert_eq!(decision(&kept_again).2.state.own_hand.len(), 6);
        assert_eq!(
            decision(&kept_again)
                .2
                .london_mulligans
                .as_ref()
                .unwrap()
                .counts[if actor == PlayerSeatV1::P0 { 0 } else { 1 }],
            1
        );
        let final_keep = step(&kept_again, "final_keep", "Keep this hand");
        let priority = server.handle_request(final_keep);
        assert!(decision(&priority)
            .2
            .london_mulligans
            .as_ref()
            .unwrap()
            .kept
            .iter()
            .all(|kept| *kept));
        assert!(decision(&priority)
            .2
            .actions
            .iter()
            .any(|action| action.label == "Pass priority"));
        let active = server.active.as_ref().unwrap();
        assert!(active
            .core
            .akroma_fixture_state()
            .engine
            .combat
            .foundations_v1
            .is_some());
        assert!(matches!(
            active.core.current_response(),
            FastActorResponseV1::Decision(_)
        ));
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn actual_action_retry_restore_and_reset_cannot_revive_a_previous_prompt_both_seats() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut server = LimitedCommonJsonlServerV2::new();
        let opening = server.handle_request(reset(actor, "reset"));
        let saved = server.handle_request(LimitedCommonRequestV2::Snapshot {
            schema_version: SCHEMA_VERSION,
            request_id: "save".into(),
        });
        let LimitedCommonBodyV2::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = saved.body
        else {
            panic!("saved");
        };
        let answer = step(&opening, "answer", "Keep this hand");
        let accepted = server.handle_request(answer.clone());
        assert_eq!(server.handle_request(answer.clone()), accepted);
        let _ = server.handle_request(request("current"));
        let mut retry = answer.clone();
        if let LimitedCommonRequestV2::Step { request_id, .. } = &mut retry {
            *request_id = "receipt_retry".into();
        }
        let retried = server.handle_request(retry);
        let LimitedCommonBodyV2::State {
            receipt: Some(receipt),
            ..
        } = retried.body
        else {
            panic!("receipt");
        };
        assert_eq!(receipt.prompt_seq, decision(&opening).2.prompt_seq);
        for iteration in 0..3 {
            let restored = server.handle_request(LimitedCommonRequestV2::Restore {
                schema_version: SCHEMA_VERSION,
                request_id: format!("restore{iteration}"),
                snapshot_id,
            });
            assert!(decision(&restored).1 > decision(&opening).1);
            let before = server
                .active
                .as_ref()
                .unwrap()
                .core
                .privileged_core_environment_hash();
            let mut old = answer.clone();
            if let LimitedCommonRequestV2::Step { request_id, .. } = &mut old {
                *request_id = format!("old{iteration}");
            }
            error(&server.handle_request(old), "stale_decision");
            assert_eq!(
                server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .privileged_core_environment_hash(),
                before
            );
            let mut wrong_actor = step(&restored, &format!("foreign{iteration}"), "Keep this hand");
            if let LimitedCommonRequestV2::Step { actor, .. } = &mut wrong_actor {
                *actor = if *actor == PlayerSeatV1::P0 {
                    PlayerSeatV1::P1
                } else {
                    PlayerSeatV1::P0
                };
            }
            error(&server.handle_request(wrong_actor), "stale_decision");
            assert_eq!(
                server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .privileged_core_environment_hash(),
                before
            );
        }
        let _ = server.handle_request(reset(actor, "new_session_same_episode"));
        error(
            &server.handle_request(LimitedCommonRequestV2::Restore {
                schema_version: SCHEMA_VERSION,
                request_id: "foreign_snapshot".into(),
                snapshot_id,
            }),
            "unknown_snapshot",
        );
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn restored_private_origin_and_ordered_family_binding_forgeries_refuse_before_action() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut server = LimitedCommonJsonlServerV2::new();
        let opening = server.handle_request(reset(actor, "reset"));
        let answer = step(&opening, "answer", "Keep this hand");
        // Full authority remains private and never arrives through JSON. This
        // tests its rederivation rather than trusting the safe exported label.
        let bound = server.pending.as_mut().unwrap();
        bound.expected.environment_revision += 1;
        let before = server
            .active
            .as_ref()
            .unwrap()
            .core
            .privileged_core_environment_hash();
        error(&server.handle_request(answer), "changed_binding");
        assert_eq!(
            server
                .active
                .as_ref()
                .unwrap()
                .core
                .privileged_core_environment_hash(),
            before
        );
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn registered_combat_primitive_actual_scan_damage_range_and_owning_restore_both_seats() {
    use crate::engine::{self, Decision};
    use crate::policy_observation_v6::tests::put;
    use crate::state::{Step, Zone};
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut server = LimitedCommonJsonlServerV2::new();
        let mut reply = server.handle_request(reset(actor, "reset"));
        for index in 0..2 {
            reply = server.handle_request(step(&reply, &format!("keep{index}"), "Keep this hand"));
        }
        // Registered physical setup, followed by real declaration/priority/
        // assignment actions. No substitute spell root or admission hook.
        let mut state = server
            .active
            .as_ref()
            .unwrap()
            .core
            .fake_fixture_state()
            .clone();
        let player = match actor {
            crate::rl::PlayerSeatV1::P0 => crate::ids::PlayerId::P0,
            crate::rl::PlayerSeatV1::P1 => crate::ids::PlayerId::P1,
        };
        let attacker = put(&mut state, player, "Spinewoods Paladin", Zone::Battlefield);
        let blocker = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.objects.get_mut(attacker).summoning_sick = false;
        state.step = Step::DeclareAttackers;
        state.priority_player = player;
        state.engine.priority_passes = [false, false];
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::DeclareAttackers { .. }
        ));
        // Let the actual common incremental scan perform this declaration.
        server.active.as_mut().unwrap().core =
            FastActorSessionV1::from_fake_v50_fixture_state(state);
        server.pending = None;
        reply = server.handle_request(request("attack_scan"));
        assert!(decision(&reply)
            .2
            .actions
            .iter()
            .any(|row| row.label.starts_with("Attack with")));
        reply = server.handle_request(step(&reply, "attack", "Attack with"));
        let mut restored_range = false;
        let mut saw_blocker = false;
        let mut finished = false;
        for index in 0..64 {
            let menu = decision(&reply).2;
            let is_range = menu
                .actions
                .iter()
                .any(|row| row.label.starts_with("Assign "));
            if is_range
                && menu
                    .actions
                    .iter()
                    .any(|row| row.label.contains(" damage to "))
                && !restored_range
            {
                let saved = server.handle_request(LimitedCommonRequestV2::Snapshot {
                    schema_version: SCHEMA_VERSION,
                    request_id: "combat_save".into(),
                });
                let LimitedCommonBodyV2::State {
                    snapshot_id: Some(id),
                    ..
                } = saved.body
                else {
                    panic!("saved range");
                };
                let old = step(&reply, "old_range", "Assign ");
                let applied = server.handle_request(old.clone());
                assert!(matches!(
                    applied.body,
                    LimitedCommonBodyV2::State {
                        receipt: Some(_),
                        ..
                    }
                ));
                reply = server.handle_request(LimitedCommonRequestV2::Restore {
                    schema_version: SCHEMA_VERSION,
                    request_id: "combat_restore".into(),
                    snapshot_id: id,
                });
                let before = server
                    .active
                    .as_ref()
                    .unwrap()
                    .core
                    .privileged_core_environment_hash();
                error(&server.handle_request(old), "stale_decision");
                assert_eq!(
                    server
                        .active
                        .as_ref()
                        .unwrap()
                        .core
                        .privileged_core_environment_hash(),
                    before
                );
                restored_range = true;
            }
            let menu = decision(&reply).2;
            let label =
                if let Some(row) = menu.actions.iter().find(|row| {
                    row.label.starts_with("Assign ") && row.label.contains(" to block ")
                }) {
                    saw_blocker = true;
                    row.label.clone()
                } else if let Some(row) = menu
                    .actions
                    .iter()
                    .find(|row| row.label.contains(" damage to "))
                {
                    row.label.clone()
                } else {
                    "Pass priority".into()
                };
            reply = server.handle_request(step(&reply, &format!("combat{index}"), &label));
            let state = server.active.as_ref().unwrap().core.fake_fixture_state();
            if state.engine.event_history.iter().any(|event| matches!(event, crate::event::CommittedEvent::Damage { source, .. } if *source == attacker)) {
                assert!(saw_blocker && restored_range);
                assert!(state.objects.get(blocker).damage > 0 || state.objects.get(blocker).zone == Zone::Graveyard);
                finished = true;
                break;
            }
        }
        // Do not silently accept an unexercised range/restore path.
        assert!(finished, "actual Foundations combat did not finish");
    }
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn public_v2_full_fake_reference_reset_owned_restore_and_continued_gameplay_both_seats() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut server = LimitedCommonJsonlServerV2::new();
        let mut input = reset(actor, "fake_reference_reset");
        if let LimitedCommonRequestV2::Reset { decks, .. } = &mut input {
            *decks = std::array::from_fn(|_| CustomDeckV1 {
                cards: crate::fake_full_engine_admission_v1_tests::reference_rows(),
            });
        }
        let opening = server.handle_request(input);
        assert_eq!(decision(&opening).2.actor, actor);
        assert_eq!(decision(&opening).2.state.own_hand.len(), 7);
        let original = step(&opening, "stale_fake_reference_keep", "Keep this hand");
        let saved = server.handle_request(LimitedCommonRequestV2::Snapshot {
            schema_version: SCHEMA_VERSION,
            request_id: "fake_reference_snapshot".into(),
        });
        let LimitedCommonBodyV2::State {
            snapshot_id: Some(snapshot),
            ..
        } = saved.body
        else {
            panic!("public V2 reference snapshot")
        };
        let mut reply = opening;
        for index in 0..2 {
            reply = server.handle_request(step(
                &reply,
                &format!("reference_keep{index}"),
                "Keep this hand",
            ));
        }
        reply = server.handle_request(step(&reply, "reference_priority", "Pass priority"));
        let core = &server.active.as_ref().unwrap().core;
        let continued = (
            serde_json::to_vec(core.fake_fixture_state()).unwrap(),
            core.policy_step_count(),
            core.physical_decision_count(),
        );
        let old_generation = decision(&reply).1;
        reply = server.handle_request(LimitedCommonRequestV2::Restore {
            schema_version: SCHEMA_VERSION,
            request_id: "fake_reference_restore".into(),
            snapshot_id: snapshot,
        });
        assert!(decision(&reply).1 > old_generation);
        let before = server
            .active
            .as_ref()
            .unwrap()
            .core
            .privileged_core_environment_hash();
        error(&server.handle_request(original), "stale_decision");
        assert_eq!(
            server
                .active
                .as_ref()
                .unwrap()
                .core
                .privileged_core_environment_hash(),
            before
        );
        reply = server.handle_request(request("fake_reference_current"));
        for index in 0..2 {
            reply = server.handle_request(step(
                &reply,
                &format!("reference_replay_keep{index}"),
                "Keep this hand",
            ));
        }
        reply = server.handle_request(step(&reply, "reference_replay_priority", "Pass priority"));
        let core = &server.active.as_ref().unwrap().core;
        assert_eq!(
            (
                serde_json::to_vec(core.fake_fixture_state()).unwrap(),
                core.policy_step_count(),
                core.physical_decision_count()
            ),
            continued
        );
        let bound = server.pending.as_ref().unwrap();
        assert!(core
            .fake_current_decision_input_v50(bound.expected, bound.expected.acting_player)
            .is_ok());
        assert!(!decision(&reply).2.actions.is_empty());
        let text = serde_json::to_string(&reply).unwrap();
        for private in [
            "arena_id",
            "event_history",
            "source_contract",
            "private_snapshot_json",
        ] {
            assert!(!text.contains(private));
        }
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn full_ravenous_deck_still_refuses_legacy_v2_publication_both_seats() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut server = LimitedCommonJsonlServerV2::new();
        let mut input = reset(actor, "full_ravenous_requires_v3");
        if let LimitedCommonRequestV2::Reset { decks, .. } = &mut input {
            // A legal Limited deck makes the unsupported printing visible in
            // either real opening hand without inserting a privileged state.
            *decks = [deck("Ravenous Amulet", 40), deck("Ravenous Amulet", 40)];
        }
        let reply = server.handle_request(input);
        let LimitedCommonBodyV2::State { state, .. } = reply.body else {
            panic!("engine Full passes deck admission before legacy refusal")
        };
        assert!(matches!(
            state.as_ref(),
            LimitedCommonStateV2::Terminal {
                classification: crate::rl::TerminalClassificationV1::Halted,
                code: crate::rl::TerminalSafeCodeV2::FailClosed,
                policy_steps: 0,
                physical_decisions: 0,
                ..
            }
        ));
        let FastActorResponseV1::Terminal(terminal) =
            server.active.as_ref().unwrap().core.current_response()
        else {
            panic!("legacy V50 backend must retain its refusal")
        };
        assert!(terminal
            .terminal_reason
            .starts_with("fail_closed:ravenous_v51_required:"));
        assert!(server.pending.is_none());
    }
}
