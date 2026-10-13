use super::*;

fn request(id: &str) -> LimitedCommonRequestV1 {
    LimitedCommonRequestV1::Current {
        schema_version: SCHEMA_VERSION,
        request_id: id.into(),
    }
}
fn decision(reply: &LimitedCommonReplyV1) -> (u64, u64, &LimitedCommonDecisionV1) {
    let LimitedCommonBodyV1::State { state, .. } = &reply.body else {
        panic!("state: {reply:?}");
    };
    let LimitedCommonStateV1::Decision {
        episode_id,
        authority_generation,
        decision,
    } = state.as_ref()
    else {
        panic!("decision: {reply:?}");
    };
    (*episode_id, *authority_generation, decision)
}
fn step(reply: &LimitedCommonReplyV1, id: &str, text: &str) -> LimitedCommonRequestV1 {
    let (episode_id, authority_generation, menu) = decision(reply);
    let action = menu
        .actions
        .iter()
        .find(|action| action.label.starts_with(text))
        .expect(text);
    LimitedCommonRequestV1::Step {
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
fn reset(actor: PlayerSeatV1, id: &str) -> LimitedCommonRequestV1 {
    LimitedCommonRequestV1::Reset {
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
fn error(reply: &LimitedCommonReplyV1, expected: &str) {
    assert_eq!(
        reply.body,
        LimitedCommonBodyV1::Error {
            code: expected.into()
        }
    );
}

#[test]
fn strict_requests_catalog_mismatch_and_unsupported_decks_do_not_replace_session() {
    let mut server = LimitedCommonJsonlServerV1::new();
    let duplicate = server.handle_line(r#"{"request_type":"current","schema_version":1,"schema_version":1,"request_id":"duplicate"}"#);
    assert!(duplicate.contains("invalid_request"));
    let unknown = server.handle_line(
        r#"{"request_type":"current","schema_version":1,"request_id":"unknown","observer":"p1"}"#,
    );
    assert!(unknown.contains("invalid_request"));
    let mut bad = reset(PlayerSeatV1::P0, "catalog");
    if let LimitedCommonRequestV1::Reset {
        expected_card_db_hash,
        ..
    } = &mut bad
    {
        *expected_card_db_hash ^= 1;
    }
    error(&server.handle_request(bad), "catalog_mismatch");
    assert!(server.active.is_none());
    let mut bad = reset(PlayerSeatV1::P0, "missing");
    if let LimitedCommonRequestV1::Reset { decks, .. } = &mut bad {
        decks[0] = deck("A card absent from every kernel catalog", 40);
    }
    error(&server.handle_request(bad), "unsupported_deck");
    assert!(server.active.is_none());
}

#[cfg(not(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
)))]
#[test]
fn default_and_standard_reset_refuse_the_new_limited_protocol() {
    let mut server = LimitedCommonJsonlServerV1::new();
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
        let mut server = LimitedCommonJsonlServerV1::new();
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
        let mut server = LimitedCommonJsonlServerV1::new();
        let opening = server.handle_request(reset(actor, "reset"));
        let saved = server.handle_request(LimitedCommonRequestV1::Snapshot {
            schema_version: 1,
            request_id: "save".into(),
        });
        let LimitedCommonBodyV1::State {
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
        if let LimitedCommonRequestV1::Step { request_id, .. } = &mut retry {
            *request_id = "receipt_retry".into();
        }
        let retried = server.handle_request(retry);
        let LimitedCommonBodyV1::State {
            receipt: Some(receipt),
            ..
        } = retried.body
        else {
            panic!("receipt");
        };
        assert_eq!(receipt.prompt_seq, decision(&opening).2.prompt_seq);
        for iteration in 0..3 {
            let restored = server.handle_request(LimitedCommonRequestV1::Restore {
                schema_version: 1,
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
            if let LimitedCommonRequestV1::Step { request_id, .. } = &mut old {
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
            if let LimitedCommonRequestV1::Step { actor, .. } = &mut wrong_actor {
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
            &server.handle_request(LimitedCommonRequestV1::Restore {
                schema_version: 1,
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
        let mut server = LimitedCommonJsonlServerV1::new();
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
#[ignore = "requires root registration of the four prepared printed sources"]
fn registered_four_printed_families_use_the_actual_catalog_and_common_reset_both_seats() {
    for name in [
        "Herald of Eternal Dawn",
        "Akroma's Memorial",
        "Fishing Pole",
        "Twinflame Tyrant",
    ] {
        for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
            let mut server = LimitedCommonJsonlServerV1::new();
            let mut request = reset(actor, "printed_reset");
            if let LimitedCommonRequestV1::Reset { decks, .. } = &mut request {
                *decks = [deck(name, 40), deck("Forest", 40)];
            }
            let opening = server.handle_request(request);
            assert_eq!(opening.card_db_hash, KERNEL_CARDDB_HASH);
            assert_eq!(opening.card_count, CARD_DEFS.len());
            assert_eq!(decision(&opening).2.state.own_hand.len(), 7);
            assert!(!decision(&opening).2.state.extensions.families.is_empty());
        }
    }
}
