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
fn ordinary_current_common_state_and_labels_match_the_existing_fixed_seat_projector() {
    use crate::human_bo3_v1::{HumanAkromaProjectorV40, HumanAkromaSessionV40};
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::state::Zone;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let old_core = FastActorSessionV1::from_akroma_v40_fixture_state(state.clone());
        let FastActorResponseV1::Decision(expected) = old_core.current_response() else {
            panic!("ordinary menu");
        };
        let old_session = HumanAkromaSessionV40::from_session(old_core, actor.into()).unwrap();
        let old = HumanAkromaProjectorV40::new(actor.into())
            .project_current(&old_session, expected)
            .unwrap();
        let mut server = LimitedCommonJsonlServerV1 {
            active: Some(Active {
                core: FastActorSessionV1::from_limited_common_fixture_state(state),
                owner: Arc::new(()),
            }),
            generation: 1,
            ..Default::default()
        };
        let reply = server.handle_request(request("ordinary"));
        let current = decision(&reply).2;
        assert_eq!(current.state, old.state);
        assert_eq!(current.actions, old.actions);
        assert!(!serde_json::to_string(current)
            .unwrap()
            .contains("Lightning Bolt"));
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
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

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn real_paid_herald_common_public_menu_restore_and_stale_binding_both_seats() {
    for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
        let (state, source) = crate::game_outcome_protection_v1::tests::cast_herald(actor);
        assert_eq!(
            state.objects.get(source).zone,
            crate::state::Zone::Battlefield
        );
        let mut server = LimitedCommonJsonlServerV1 {
            active: Some(Active {
                core: FastActorSessionV1::from_limited_common_fixture_state(state),
                owner: Arc::new(()),
            }),
            generation: 1,
            ..Default::default()
        };
        let opening = server.handle_request(request("paid_public"));
        let json = serde_json::to_string(&opening).unwrap();
        assert!(json.contains("Herald of Eternal Dawn"));
        assert!(json.contains("cannot_lose"));
        let save = server.handle_request(LimitedCommonRequestV1::Snapshot {
            schema_version: 1,
            request_id: "save_paid".into(),
        });
        let LimitedCommonBodyV1::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = save.body
        else {
            panic!("snapshot");
        };
        let pass = step(&opening, "pass", "Pass priority");
        let accepted = server.handle_request(pass.clone());
        assert!(matches!(
            accepted.body,
            LimitedCommonBodyV1::State {
                receipt: Some(_),
                ..
            }
        ));
        let restored = server.handle_request(LimitedCommonRequestV1::Restore {
            schema_version: 1,
            request_id: "restore_paid".into(),
            snapshot_id,
        });
        assert_eq!(decision(&restored).2.state, decision(&opening).2.state);
        let before = server
            .active
            .as_ref()
            .unwrap()
            .core
            .privileged_core_environment_hash();
        error(&server.handle_request(pass), "stale_decision");
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
        let mut server = LimitedCommonJsonlServerV1::new();
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
            .akroma_fixture_state()
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
            FastActorSessionV1::from_limited_common_fixture_state(state);
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
                let saved = server.handle_request(LimitedCommonRequestV1::Snapshot {
                    schema_version: 1,
                    request_id: "combat_save".into(),
                });
                let LimitedCommonBodyV1::State {
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
                    LimitedCommonBodyV1::State {
                        receipt: Some(_),
                        ..
                    }
                ));
                reply = server.handle_request(LimitedCommonRequestV1::Restore {
                    schema_version: 1,
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
            let state = server.active.as_ref().unwrap().core.akroma_fixture_state();
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
