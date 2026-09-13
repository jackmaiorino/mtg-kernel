use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::{GameState, Step, Zone};

fn decision(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(decision) => decision,
        other => panic!("fixture requires a decision: {other:?}"),
    }
}

fn basic_state(actor: PlayerId, permuted: bool, duplicate_land: bool) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    // The permutation reverses visible allocations, not just their offset.
    // Both zone arrays follow their newly allocated producer order.
    let mut inventory = vec![
        (actor, "Mountain", Zone::Battlefield),
        (
            actor.opponent(),
            if permuted {
                "Flaring Pain"
            } else {
                "Fireblast"
            },
            Zone::Hand,
        ),
        (actor, "Lightning Bolt", Zone::Hand),
        (actor, "Mountain", Zone::Hand),
        (actor, "Forest", Zone::Battlefield),
        (
            actor.opponent(),
            if permuted {
                "Fireblast"
            } else {
                "Flaring Pain"
            },
            Zone::Library,
        ),
    ];
    if duplicate_land {
        inventory.push((actor, "Mountain", Zone::Hand));
    }
    if permuted {
        inventory.reverse();
    }
    for (owner, name, zone) in inventory {
        let id = put(&mut state, owner, name, zone);
        // A hidden incarnation counter also cannot become a sort key.
        if permuted {
            state.objects.get_mut(id).zone_change_count = id.0 * 17 + 3;
        }
    }
    state
}

fn prompt(state: GameState, seat: PlayerSeatV1) -> HumanDecisionV1 {
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    HumanDecisionProjectorV1::new(seat)
        .project_current(&session, decision(&session))
        .unwrap()
}

fn assert_no_private_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                assert!(
                    !matches!(
                        key.as_str(),
                        "arena_id"
                            | "zone_change_count"
                            | "zone_change_generation"
                            | "visible_projection_hash"
                            | "physical_decision_id"
                            | "environment_revision"
                            | "episode_id"
                            | "seed"
                            | "timestamp"
                            | "binding"
                            | "stable_id"
                            | "engine_action_indexes"
                            | "logits"
                            | "value"
                    ),
                    "private key {key}"
                );
                assert_no_private_keys(child);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                assert_no_private_keys(child);
            }
        }
        _ => {}
    }
}

#[test]
fn human_prompt_nonmonotonic_renumbering_and_hidden_sentinels_are_invariant() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let baseline = prompt(basic_state(actor, false, false), actor.into());
        let renamed = prompt(basic_state(actor, true, false), actor.into());
        assert_eq!(baseline, renamed);
        let value = serde_json::to_value(&baseline).unwrap();
        assert_no_private_keys(&value);
        let text = value.to_string();
        assert!(!text.contains("Fireblast"));
        assert!(!text.contains("Flaring Pain"));
        assert!(text.contains("Lightning Bolt"));
        assert_eq!(baseline.state.own_hand.len(), 2);
    }
}

#[test]
fn human_duplicate_opening_lands_keep_distinct_choices_without_hidden_order() {
    let baseline = prompt(basic_state(PlayerId::P0, false, true), PlayerSeatV1::P0);
    let renamed = prompt(basic_state(PlayerId::P0, true, true), PlayerSeatV1::P0);
    assert_eq!(baseline, renamed);
    let lands: Vec<_> = baseline
        .actions
        .iter()
        .filter(|action| action.label.starts_with("Play Mountain"))
        .collect();
    assert_eq!(lands.len(), 2);
    assert_ne!(lands[0].label, lands[1].label);
    assert_eq!(baseline.state.own_hand.len(), 3);
}

#[test]
fn human_action_mapping_executes_exact_choice_and_retries_once() {
    let mut state = basic_state(PlayerId::P0, false, true);
    put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    let mut session = FastActorSessionV1::from_v3_fixture_state(state.clone());
    let mut reference = FastActorSessionV1::from_v3_fixture_state(state);
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let expected = decision(&session);
    let before = session.privileged_core_environment_hash();
    let visible = adapter.project_current(&session, expected).unwrap();
    assert_eq!(
        visible,
        adapter.project_current(&session, expected).unwrap()
    );
    assert_eq!(before, session.privileged_core_environment_hash());
    let selected = visible
        .actions
        .iter()
        .find(|action| action.label.starts_with("Play Forest"))
        .unwrap()
        .action_index;
    let (_, original_actions, reference_binding) = reference
        .human_current_decision_input_v1(decision(&reference), PlayerSeatV1::P0)
        .unwrap();
    // Independent semantic oracle: do not derive the reference action from
    // the adapter's own map, which would let the same wrong map pass twice.
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let engine_index = original_actions
        .iter()
        .position(|action| {
            matches!(action,
        crate::rl::ActionSemanticV1::PlayLand { source, .. } if source.card_db_id == forest)
        })
        .unwrap() as u32;
    reference
        .consume_current_flat_action_slice_v3(reference_binding, engine_index)
        .unwrap();
    let request = HumanActionRequestV1 {
        prompt_seq: visible.prompt_seq,
        action_index: selected,
    };
    let receipt = adapter.submit(&mut session, request.clone()).unwrap();
    assert_eq!(
        session.privileged_core_environment_hash(),
        reference.privileged_core_environment_hash()
    );
    assert_eq!(session.game_state().players[0].lands_played_this_turn, 1);
    assert_eq!(
        session.game_state().players[0]
            .battlefield
            .iter()
            .filter(|id| session.game_state().objects.get(**id).card_def == forest)
            .count(),
        2
    );
    let after = session.privileged_core_environment_hash();
    assert_eq!(
        adapter.submit(&mut session, request.clone()).unwrap(),
        receipt
    );
    assert_eq!(session.privileged_core_environment_hash(), after);
    let changed = HumanActionRequestV1 {
        action_index: selected + 1,
        ..request
    };
    assert_eq!(
        adapter.submit(&mut session, changed),
        Err(HumanDecisionErrorV1::ConflictingRetry)
    );
    assert_eq!(session.privileged_core_environment_hash(), after);
    assert_no_private_keys(&serde_json::to_value(receipt).unwrap());
}

#[test]
fn human_invalid_and_stale_requests_do_not_advance_state_or_rng() {
    let mut session =
        FastActorSessionV1::from_v3_fixture_state(basic_state(PlayerId::P0, false, false));
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let visible = adapter
        .project_current(&session, decision(&session))
        .unwrap();
    let before = session.privileged_core_environment_hash();
    assert_eq!(
        adapter.submit(
            &mut session,
            HumanActionRequestV1 {
                prompt_seq: visible.prompt_seq,
                action_index: u32::MAX
            }
        ),
        Err(HumanDecisionErrorV1::InvalidAction)
    );
    assert_eq!(before, session.privileged_core_environment_hash());
    assert_eq!(
        adapter.submit(
            &mut session,
            HumanActionRequestV1 {
                prompt_seq: visible.prompt_seq + 1,
                action_index: 0
            }
        ),
        Err(HumanDecisionErrorV1::StaleDecision)
    );
    assert_eq!(before, session.privileged_core_environment_hash());
    let pending = adapter.pending.as_ref().unwrap();
    session
        .consume_current_flat_action_slice_v3(pending.binding, pending.engine_action_indexes[0])
        .unwrap();
    let after_external_action = session.privileged_core_environment_hash();
    assert_eq!(
        adapter.submit(
            &mut session,
            HumanActionRequestV1 {
                prompt_seq: visible.prompt_seq,
                action_index: 0
            }
        ),
        Err(HumanDecisionErrorV1::StaleDecision)
    );
    assert_eq!(
        after_external_action,
        session.privileged_core_environment_hash()
    );
}

#[test]
fn human_fixed_seat_rejects_opponent_view_and_forged_actor() {
    let session =
        FastActorSessionV1::from_v3_fixture_state(basic_state(PlayerId::P1, false, false));
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let expected = decision(&session);
    let before = session.privileged_core_environment_hash();
    assert_eq!(
        adapter.project_current(&session, expected),
        Err(HumanDecisionErrorV1::NotHumanTurn)
    );
    let mut forged = expected;
    forged.acting_player = PlayerSeatV1::P0;
    assert_eq!(
        adapter.project_current(&session, forged),
        Err(HumanDecisionErrorV1::StaleDecision)
    );
    assert_eq!(before, session.privileged_core_environment_hash());
    assert!(adapter.pending.is_none());
}

#[test]
fn human_unsupported_ability_rejects_complete_prompt_before_publication() {
    let mut state = ready_state();
    put(
        &mut state,
        PlayerId::P0,
        "Timberwatch Elf",
        Zone::Battlefield,
    );
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let before = session.privileged_core_environment_hash();
    assert_eq!(
        adapter.project_current(&session, decision(&session)),
        Err(HumanDecisionErrorV1::UnsupportedPrompt)
    );
    assert!(adapter.pending.is_none());
    assert_eq!(adapter.next_prompt_seq, 1);
    assert_eq!(before, session.privileged_core_environment_hash());
    assert_eq!(
        serde_json::to_string(&HumanDecisionErrorV1::UnsupportedPrompt).unwrap(),
        "\"unsupported_prompt\""
    );
}

#[test]
fn human_search_candidates_are_unordered_visible_choices_without_hidden_positions() {
    use crate::policy_observation_v6::tests::{
        forest_search_state, forest_search_state_with_hidden_renumbering,
    };
    let first = prompt(
        forest_search_state(false, "Lightning Bolt"),
        PlayerSeatV1::P0,
    );
    let reordered = prompt(forest_search_state(true, "Fireblast"), PlayerSeatV1::P0);
    let renamed = prompt(
        forest_search_state_with_hidden_renumbering(),
        PlayerSeatV1::P0,
    );
    assert_eq!(first, reordered);
    assert_eq!(first, renamed);
    let search = first
        .state
        .extensions
        .decision_local_library
        .as_ref()
        .unwrap();
    assert_eq!(search.cards.len(), 3);
    assert!(first.state.known_library_cards.iter().all(Vec::is_empty));
    let text = serde_json::to_string(&first).unwrap();
    assert!(!text.contains("Lightning Bolt"));
    assert!(!text.contains("Fireblast"));
    assert_no_private_keys(&serde_json::to_value(first).unwrap());
}

#[test]
fn human_ward_label_binds_exact_public_stack_item_and_payment() {
    use crate::policy_observation_v6::tests::{reach_ward_payment, ward_multi_targeter_state};
    let (mut state, _, _) = ward_multi_targeter_state();
    reach_ward_payment(&mut state);
    let visible = prompt(state, PlayerSeatV1::P0);
    let ward = visible
        .state
        .extensions
        .pending_ward_payment
        .as_ref()
        .unwrap();
    assert_eq!(visible.actions.len(), 2);
    let stack_label = format!("stack item #{}", ward.targeting_stack_index + 1);
    assert!(visible
        .actions
        .iter()
        .all(|action| action.label.contains(&stack_label) && action.label.contains("{2}")));
    assert!(visible.actions.iter().any(|action| action
        .label
        .contains("Ward will attempt to counter that item")));
    assert_no_private_keys(&serde_json::to_value(visible).unwrap());
}

#[test]
fn human_actual_inclusion_prompt_preserves_public_candidate_order_under_renumbering() {
    fn state(reverse: bool) -> GameState {
        let mut state = ready_state();
        let order = if reverse {
            ["Goblin Bushwhacker", "Voldaren Epicure"]
        } else {
            ["Voldaren Epicure", "Goblin Bushwhacker"]
        };
        let mut ids = Vec::new();
        for name in order {
            ids.push((name, put(&mut state, PlayerId::P0, name, Zone::Battlefield)));
        }
        // Preserve the same visible entry/declaration order while changing
        // arena allocation in the opposite direction.
        state.players[0].battlefield = ["Voldaren Epicure", "Goblin Bushwhacker"]
            .iter()
            .map(|name| {
                ids.iter()
                    .find(|(candidate, _)| candidate == name)
                    .unwrap()
                    .1
            })
            .collect();
        state.step = Step::DeclareAttackers;
        state
    }
    let first = prompt(state(false), PlayerSeatV1::P0);
    let renamed = prompt(state(true), PlayerSeatV1::P0);
    assert_eq!(first, renamed);
    assert!(first
        .state
        .policy_context
        .private_combat_selection
        .is_some());
    assert!(first
        .actions
        .iter()
        .any(|action| action.label.starts_with("Attack with")));
}

#[test]
fn human_requests_reject_unknown_fields_and_cannot_request_an_observer() {
    assert!(serde_json::from_str::<HumanActionRequestV1>(
        r#"{"prompt_seq":1,"action_index":0,"human_seat":"P1"}"#
    )
    .is_err());
    assert!(serde_json::from_str::<HumanActionRequestV1>(
        r#"{"prompt_seq":1,"action_index":0,"seed":1}"#
    )
    .is_err());
    assert!(
        serde_json::from_str::<HumanActionRequestV1>(r#"{"prompt_seq":1,"action_index":-1}"#)
            .is_err()
    );
}

#[test]
fn human_current_binding_remains_valid_for_direct_supported_mana_choice() {
    let state = basic_state(PlayerId::P0, false, false);
    let mut session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let visible = adapter
        .project_current(&session, decision(&session))
        .unwrap();
    let choice = visible
        .actions
        .iter()
        .find(|action| action.label.contains("Tap Mountain") && action.label.contains("add 1 {R}"))
        .unwrap();
    adapter
        .submit(
            &mut session,
            HumanActionRequestV1 {
                prompt_seq: visible.prompt_seq,
                action_index: choice.action_index,
            },
        )
        .unwrap();
    assert_eq!(
        session.game_state().players[0].mana_pool[crate::mana::ManaColor::R.pool_index()],
        1
    );
}

#[test]
fn human_equipment_synthetic_timestamps_do_not_reveal_allocation_order() {
    fn state(reverse: bool) -> GameState {
        let mut state = ready_state();
        // Keep a real P0 choice available so forced priority passing does not
        // advance to an opponent equipment ability before projection.
        put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
        let mut names = vec![
            "Black Mage's Rod",
            "Voldaren Epicure",
            "Viridian Longbow",
            "Goblin Bushwhacker",
        ];
        if reverse {
            names.reverse();
        }
        let ids: Vec<_> = names
            .into_iter()
            .map(|name| (name, put(&mut state, PlayerId::P1, name, Zone::Battlefield)))
            .collect();
        let lookup = |name| {
            ids.iter()
                .find(|(candidate, _)| *candidate == name)
                .unwrap()
                .1
        };
        state
            .attach_object_exact(lookup("Black Mage's Rod"), 0, lookup("Voldaren Epicure"), 0)
            .unwrap();
        state
            .attach_object_exact(
                lookup("Viridian Longbow"),
                0,
                lookup("Goblin Bushwhacker"),
                0,
            )
            .unwrap();
        state
    }
    let first = prompt(state(false), PlayerSeatV1::P0);
    let renamed = prompt(state(true), PlayerSeatV1::P0);
    assert_eq!(first, renamed);
    assert_eq!(first.state.public.continuous_effects.len(), 2);
    assert!(first
        .state
        .public
        .continuous_effects
        .iter()
        .all(|effect| effect.timestamp_order.is_none()));
}

#[test]
fn human_same_name_hosts_with_multiple_attachments_use_visible_ordering() {
    fn state(reverse: bool) -> GameState {
        let mut state = ready_state();
        put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
        let mut entries = vec![
            ("a", "Goblin Bushwhacker"),
            ("rod", "Black Mage's Rod"),
            ("bow", "Viridian Longbow"),
            ("b", "Goblin Bushwhacker"),
            ("blowgun", "Hunter's Blowgun"),
        ];
        if reverse {
            entries.reverse();
        }
        let ids: Vec<_> = entries
            .into_iter()
            .map(|(label, name)| {
                (
                    label,
                    put(&mut state, PlayerId::P1, name, Zone::Battlefield),
                )
            })
            .collect();
        let lookup = |label| {
            ids.iter()
                .find(|(candidate, _)| *candidate == label)
                .unwrap()
                .1
        };
        state
            .attach_object_exact(lookup("rod"), 0, lookup("a"), 0)
            .unwrap();
        state
            .attach_object_exact(lookup("bow"), 0, lookup("a"), 0)
            .unwrap();
        state
            .attach_object_exact(lookup("blowgun"), 0, lookup("b"), 0)
            .unwrap();
        state
    }
    assert_eq!(
        prompt(state(false), PlayerSeatV1::P0),
        prompt(state(true), PlayerSeatV1::P0)
    );
}

#[test]
fn human_projection_removes_revoked_opponent_hand_knowledge() {
    let mut state = basic_state(PlayerId::P0, false, false);
    let revealed = state.players[1].hand[0];
    state
        .reveal_hand_card(PlayerId::P0, PlayerId::P1, revealed)
        .unwrap();
    let known = prompt(state.clone(), PlayerSeatV1::P0);
    assert_eq!(known.state.known_hand_cards[1].len(), 1);
    assert_eq!(known.state.known_hand_cards[1][0].stable.name, "Fireblast");
    state.clear_nonowner_hand_knowledge(PlayerId::P1);
    let revoked = prompt(state, PlayerSeatV1::P0);
    assert!(revoked.state.known_hand_cards[1].is_empty());
    assert!(!serde_json::to_string(&revoked)
        .unwrap()
        .contains("Fireblast"));
}

#[test]
fn human_unordered_target_menu_permutation_does_not_change_prompt() {
    let state = crate::policy_observation_v6::tests::forest_search_state(false, "Lightning Bolt");
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let (mut observation, mut actions, _) = session
        .human_current_decision_input_v1(decision(&session), PlayerSeatV1::P0)
        .unwrap();
    let (first, _) =
        visible::project_decision(&observation, &actions, PlayerSeatV1::P0, 1).unwrap();
    let crate::rl::PendingEffectChoiceSemanticV4::Targets { legal_targets, .. } = observation
        .projection
        .surface
        .engine_context
        .pending_effect
        .as_mut()
        .unwrap()
        .choice
        .as_mut()
        .unwrap()
    else {
        panic!("target fixture");
    };
    legal_targets.reverse();
    actions.reverse();
    let (reordered, map) =
        visible::project_decision(&observation, &actions, PlayerSeatV1::P0, 1).unwrap();
    assert_eq!(first, reordered);
    assert_eq!(map.len(), actions.len());
}

#[test]
fn human_completed_library_search_drops_temporary_menu() {
    let state = crate::policy_observation_v6::tests::forest_search_state(false, "Lightning Bolt");
    let mut session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let first = adapter
        .project_current(&session, decision(&session))
        .unwrap();
    let forest = first
        .actions
        .iter()
        .find(|action| action.label.starts_with("Select Forest "))
        .unwrap();
    adapter
        .submit(
            &mut session,
            HumanActionRequestV1 {
                prompt_seq: first.prompt_seq,
                action_index: forest.action_index,
            },
        )
        .unwrap();
    let after = adapter
        .project_current(&session, decision(&session))
        .unwrap();
    assert!(after.state.extensions.decision_local_library.is_none());
    assert!(after
        .state
        .own_hand
        .iter()
        .any(|card| card.stable.name == "Forest"));
    assert!(after.state.known_library_cards.iter().all(Vec::is_empty));
}
