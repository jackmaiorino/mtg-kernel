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

fn human_impulse_exile_state(
    actor: PlayerId,
    holder: PlayerId,
    reverse: bool,
    first_expiry: Option<crate::engine::PlayPermissionExpiry>,
) -> (GameState, crate::ids::ObjectId, crate::ids::ObjectId) {
    use crate::engine::{PlayOrCast, PlayPermission, PlayPermissionExpiry};
    use crate::event::{self, ProposedEvent};
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    let mut cards = Vec::new();
    for first in if reverse { [false, true] } else { [true, false] } {
        let card = put(&mut state, holder, "Mountain", Zone::Hand);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Exile));
        if reverse {
            state.objects.get_mut(card).zone_change_count += 17 + card.0;
        }
        let expiry = if first {
            first_expiry
        } else {
            Some(PlayPermissionExpiry::UntilHoldersNextTurn { holder_turn_started: false })
        };
        if let Some(expiry) = expiry {
            state.engine.exile_play_permissions.push(PlayPermission {
                object: card,
                holder,
                zone_change_generation: state.objects.get(card).zone_change_count,
                play_or_cast: PlayOrCast::Play,
                expiry,
            });
        }
        cards.push((first, card));
    }
    let first = cards.iter().find(|(first, _)| *first).unwrap().1;
    let second = cards.iter().find(|(first, _)| !*first).unwrap().1;
    (state, first, second)
}

#[test]
fn human_expired_impulse_copy_is_distinguished_and_exact_live_land_is_played() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        // Also exercise the original smoke failure: identical opponent exile
        // cards, only one of which has a current public play permission.
        for holder in [actor, actor.opponent()] {
            let mut expected = None;
            for reverse in [false, true] {
                let (state, expired, live) = human_impulse_exile_state(actor, holder, reverse, None);
                let mut session = FastActorSessionV1::from_v3_fixture_state(state);
                let mut adapter = HumanDecisionProjectorV1::new(actor.into());
                let visible = adapter.project_current(&session, decision(&session)).unwrap();
                assert_eq!(visible.state.public.exile.len(), 2);
                assert_eq!(visible.state.public.exile_play_permissions.len(), 1);
                assert_no_private_keys(&serde_json::to_value(&visible).unwrap());
                if let Some(expected) = &expected {
                    assert_eq!(&visible, expected);
                } else {
                    expected = Some(visible.clone());
                }
                let choices: Vec<_> = visible.actions.iter()
                    .filter(|action| action.label.starts_with("Play Mountain ")).collect();
                assert_eq!(choices.len(), usize::from(holder == actor));
                if holder == actor {
                    let permission = &visible.state.public.exile_play_permissions[0];
                    assert!(choices[0].label.contains(&format!("[{}]", permission.object.handle)));
                    adapter.submit(&mut session, HumanActionRequestV1 {
                        prompt_seq: visible.prompt_seq,
                        action_index: choices[0].action_index,
                    }).unwrap();
                    assert_eq!(session.game_state().objects.get(expired).zone, Zone::Exile);
                    assert_eq!(session.game_state().objects.get(live).zone, Zone::Battlefield);
                }
            }
        }
    }
}

#[test]
fn human_equal_and_distinct_impulse_expiries_are_invariant_under_permission_reordering() {
    use crate::engine::PlayPermissionExpiry;
    for expiry in [
        PlayPermissionExpiry::EndOfTurn,
        PlayPermissionExpiry::UntilHoldersNextTurn { holder_turn_started: false },
        PlayPermissionExpiry::UntilHoldersNextTurn { holder_turn_started: true },
    ] {
        let (first, _, _) = human_impulse_exile_state(PlayerId::P0, PlayerId::P0, false, Some(expiry));
        let (renamed, _, _) = human_impulse_exile_state(PlayerId::P0, PlayerId::P0, true, Some(expiry));
        let first = prompt(first, PlayerSeatV1::P0);
        assert_eq!(first, prompt(renamed, PlayerSeatV1::P0));
        let choices: Vec<_> = first.actions.iter()
            .filter(|action| action.label.starts_with("Play Mountain ")).collect();
        assert_eq!(choices.len(), 2);
        assert_ne!(choices[0].label, choices[1].label);
        assert_no_private_keys(&serde_json::to_value(first).unwrap());
    }
}

fn human_team_haste_state(reverse: bool, token_count: usize) -> (GameState, Vec<crate::ids::ObjectId>) {
    use crate::engine::{EffectDuration, Layers, UntilEndOfTurnEffect};
    let mut state = ready_state();
    put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let mut names = vec!["Human Soldier Token"; token_count];
    names.push("Voldaren Epicure");
    if reverse { names.reverse(); }
    let mut affected = Vec::new();
    let mut tokens = Vec::new();
    for name in names {
        let object = put(&mut state, PlayerId::P1, name, Zone::Battlefield);
        state.objects.get_mut(object).summoning_sick = true;
        if reverse { state.objects.get_mut(object).zone_change_count += 13 + object.0; }
        affected.push(object);
        if name == "Human Soldier Token" { tokens.push(object); }
    }
    state.engine.until_end_of_turn.push(UntilEndOfTurnEffect::ResolvedSetEffect {
        object_ids: affected,
        layer: Layers::ABILITY_ADDING,
        timestamp: 1,
        duration: EffectDuration::EndOfTurn,
        power: 0,
        toughness: 0,
        grant_haste: true,
    });
    (state, tokens)
}

#[test]
fn human_equivalent_team_haste_tokens_preserve_prompt_and_distinct_target_choices() {
    for token_count in [2, 3] {
        for targeting in [false, true] {
            let mut expected = None;
            for reverse in [false, true] {
                let (mut state, _) = human_team_haste_state(reverse, token_count);
                if targeting {
                    let bolt = state.players[0].hand.iter().copied()
                        .find(|id| state.objects.get(*id).name == "Lightning Bolt").unwrap();
                    crate::engine::step(&mut state, crate::engine::Action::CastSpell(bolt)).unwrap();
                }
                let visible = prompt(state, PlayerSeatV1::P0);
                let tokens: Vec<_> = visible.state.public.battlefield[1].iter()
                    .filter(|card| card.stable.name == "Human Soldier Token").collect();
                assert_eq!(tokens.len(), token_count);
                assert!(tokens.iter().all(|card| card.characteristics.effective_keywords.haste));
                assert_eq!(visible.state.public.continuous_effects[0].affected_objects.len(), token_count + 1);
                if targeting {
                    let choices: Vec<_> = visible.actions.iter()
                        .filter(|action| action.label.starts_with("Target Human Soldier Token ")).collect();
                    assert_eq!(choices.len(), token_count);
                    let unique: std::collections::BTreeSet<_> = choices.iter().map(|action| &action.label).collect();
                    assert_eq!(unique.len(), token_count);
                }
                assert_no_private_keys(&serde_json::to_value(&visible).unwrap());
                if let Some(expected) = &expected {
                    assert_eq!(&visible, expected);
                } else {
                    expected = Some(visible);
                }
            }
        }
    }
}

#[test]
fn human_same_characteristics_do_not_erase_asymmetric_effect_membership() {
    use crate::engine::UntilEndOfTurnEffect;
    let (mut state, tokens) = human_team_haste_state(false, 2);
    let mut extra = state.engine.until_end_of_turn[0].clone();
    let UntilEndOfTurnEffect::ResolvedSetEffect { object_ids, timestamp, .. } = &mut extra else {
        panic!("team effect fixture");
    };
    *object_ids = vec![tokens[0]];
    *timestamp = 2;
    // Both tokens still have precisely the same characteristics, but one
    // appears in a second public effect. Do not erase that public graph fact.
    state.engine.until_end_of_turn.push(extra);
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    assert_eq!(adapter.project_current(&session, decision(&session)), Err(HumanDecisionErrorV1::UnsupportedPrompt));
    assert!(adapter.pending.is_none());
}

#[test]
fn human_effect_source_is_not_interchangeable_with_an_equal_affected_card() {
    let (state, _) = human_team_haste_state(false, 2);
    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let (mut observation, actions, _) = session
        .human_current_decision_input_v1(decision(&session), PlayerSeatV1::P0).unwrap();
    let token_id = crate::card_def::card_id_by_name("Human Soldier Token").unwrap();
    let token = observation.projection.surface.battlefield[1].iter()
        .find(|card| card.stable.card_db_id == token_id).unwrap().stable.clone();
    observation.projection.surface.continuous_effects[0].source = Some(token);
    assert_eq!(visible::project_decision(&observation, &actions, PlayerSeatV1::P0, 1),
        Err(HumanDecisionErrorV1::UnsupportedPrompt));
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

fn submit_label(
    session: &mut FastActorSessionV1,
    adapter: &mut HumanDecisionProjectorV1,
    matches: impl Fn(&str) -> bool,
) -> HumanDecisionV1 {
    let visible = adapter.project_current(session, decision(session)).unwrap();
    let action_index = visible.actions.iter().find(|action| matches(&action.label))
        .unwrap_or_else(|| panic!("expected label in {:?}", visible.actions)).action_index;
    adapter.submit(session, HumanActionRequestV1 { prompt_seq: visible.prompt_seq, action_index }).unwrap();
    visible
}

#[test]
fn human_blood_activation_names_all_costs_and_binds_the_discard() {
    let mut state = ready_state();
    let blood = put(&mut state, PlayerId::P0, "Blood Token", Zone::Battlefield);
    let mountain = put(&mut state, PlayerId::P0, "Mountain", Zone::Hand);
    put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Library);
    put(&mut state, PlayerId::P1, "Fireblast", Zone::Hand);
    state.players[0].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
    let mut session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let before = submit_label(&mut session, &mut adapter, |label| label.starts_with("Activate Blood Token"));
    let activation = before.actions.iter().find(|action| action.label.starts_with("Activate Blood Token")).unwrap();
    assert!(activation.label.contains("pay {1}"));
    assert!(activation.label.contains("tap Blood Token"));
    assert!(activation.label.contains("sacrifice Blood Token"));
    assert!(activation.label.contains("discard 1 cards"));
    assert!(activation.label.contains("draws 1 cards"));
    assert!(session.game_state().engine.pending_activation.is_some());
    assert!(session.game_state().engine.pending_discard.is_some());
    let discard = submit_label(&mut session, &mut adapter, |label| label.starts_with("Discard Mountain"));
    assert_eq!(session.game_state().objects.get(mountain).zone, Zone::Graveyard);
    assert_ne!(session.game_state().objects.get(blood).zone, Zone::Battlefield);
    assert!(session.game_state().engine.pending_activation.is_none());
    assert!(!serde_json::to_string(&discard).unwrap().contains("Fireblast"));
    assert_no_private_keys(&serde_json::to_value(&before).unwrap());
}

#[test]
fn human_synthesizer_activation_names_payment_token_and_timing() {
    let mut state = ready_state();
    let synth = put(&mut state, PlayerId::P0, "Experimental Synthesizer", Zone::Battlefield);
    put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    put(&mut state, PlayerId::P0, "Mountain", Zone::Library);
    // The harness suppresses P0's fresh-own-stack priority. Give P1 a real
    // instant response so this assertion observes the announced ability,
    // before both it and the leaves trigger are automatically resolved.
    put(&mut state, PlayerId::P1, "Lightning Bolt", Zone::Hand);
    put(&mut state, PlayerId::P1, "Mountain", Zone::Battlefield);
    state.players[0].mana_pool[crate::mana::ManaColor::R.pool_index()] = 3;
    let mut session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let visible = submit_label(&mut session, &mut adapter, |label| label.starts_with("Activate Experimental Synthesizer"));
    let activation = visible.actions.iter().find(|action| action.label.starts_with("Activate Experimental Synthesizer")).unwrap();
    assert!(activation.label.contains("pay {2}{R}"));
    assert!(activation.label.contains("sacrifice Experimental Synthesizer"));
    assert!(activation.label.contains("Samurai Token (2/2) with vigilance"));
    assert!(activation.label.contains("only as a sorcery"));
    assert_eq!(decision(&session).acting_player, PlayerSeatV1::P1);
    assert_eq!(session.game_state().objects.get(synth).zone, Zone::Graveyard);
    assert!(session.game_state().stack.iter().any(|item| matches!(&item.inline_effect,
        Some(crate::effect::EffectOp::CreateToken { token_def, .. }) if *token_def == crate::card_def::card_id_by_name("Samurai Token").unwrap())));
}

fn human_chain_payment_state() -> GameState {
    use crate::engine::{self, Action, Decision};
    use crate::state::Target;
    let mut state = ready_state();
    let chain = put(&mut state, PlayerId::P0, "Chain Lightning", Zone::Hand);
    put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Forest", Zone::Hand);
    engine::step(&mut state, Action::CastSpell(chain)).unwrap();
    engine::step(&mut state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();
    for _ in 0..16 {
        match engine::advance_until_decision(&mut state) {
            Decision::ChooseSpellCopyPayment { player: PlayerId::P1, .. } => return state,
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected Chain payment path: {other:?}"),
        }
    }
    panic!("Chain payment not reached");
}

#[test]
fn human_chain_copy_payment_and_retarget_are_distinct_bound_choices() {
    for pay in [false, true] {
        let mut session = FastActorSessionV1::from_v3_fixture_state(human_chain_payment_state());
        let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P1);
        let visible = submit_label(&mut session, &mut adapter, |label|
            label.starts_with(if pay { "Attempt to pay {R}{R}" } else { "Decline to pay {R}{R}" }));
        assert!(visible.actions.iter().all(|action| action.label.contains("inherited target: you")));
        assert_eq!(visible.state.public.life_totals[1], 17);
        if pay {
            let retarget = adapter.project_current(&session, decision(&session)).unwrap();
            assert!(retarget.actions.iter().any(|action| action.label.starts_with("Keep the inherited target")));
            assert!(retarget.actions.iter().any(|action| action.label.starts_with("Choose a new target")));
            assert!(session.game_state().engine.pending_spell_copy.as_ref().unwrap().copy_source.is_some());
            assert!(session.game_state().players[1].battlefield.iter().all(|id| session.game_state().objects.get(*id).tapped));
        } else {
            assert!(session.game_state().engine.pending_spell_copy.is_none());
            assert!(session.game_state().players[1].battlefield.iter().all(|id| !session.game_state().objects.get(*id).tapped));
        }
        assert_no_private_keys(&serde_json::to_value(visible).unwrap());
    }
}

fn human_trigger_order_state(repeated_source: bool) -> GameState {
    use crate::event::{self, ProposedEvent};
    let mut state = ready_state();
    put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    let first = put(&mut state, PlayerId::P0, "Voldaren Epicure", Zone::Hand);
    let second = put(&mut state, PlayerId::P0, "Burning-Tree Emissary", Zone::Hand);
    event::propose_and_commit_batch(&mut state, vec![
        ProposedEvent::zone_change(first, Zone::Battlefield),
        ProposedEvent::zone_change(second, Zone::Battlefield),
    ]);
    state.engine.pending_triggers = crate::trigger::collect_and_process(&mut state);
    if repeated_source {
        let repeated = state.engine.pending_triggers[0].clone();
        state.engine.pending_triggers = vec![state.engine.pending_triggers[0].clone(), repeated];
    }
    state
}

#[test]
fn human_trigger_order_uses_neutral_named_sources_and_exact_bottom_to_top_order() {
    let mut state = human_trigger_order_state(false);
    // Without an opponent response the surface auto-passes both players
    // and resolves the stack before submit returns. Keep the exact chosen
    // placement observable at a genuine P1 priority decision instead.
    put(&mut state, PlayerId::P1, "Lightning Bolt", Zone::Hand);
    put(&mut state, PlayerId::P1, "Mountain", Zone::Battlefield);
    let mut session = FastActorSessionV1::from_v3_fixture_state(state);
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    let visible = submit_label(&mut session, &mut adapter, |label|
        label.find("Burning-Tree Emissary").zip(label.find("Voldaren Epicure")).is_some_and(|(a, b)| a < b));
    assert!(visible.actions.iter().all(|action| action.label.contains("last resolves first")));
    assert_eq!(decision(&session).acting_player, PlayerSeatV1::P1);
    assert_eq!(session.game_state().stack.len(), 2);
    assert_eq!(session.game_state().objects.get(session.game_state().stack[0].source).name, "Burning-Tree Emissary");
    assert_eq!(session.game_state().objects.get(session.game_state().stack[1].source).name, "Voldaren Epicure");
}

#[test]
fn human_repeated_unlabeled_triggers_from_one_source_are_rejected() {
    let session = FastActorSessionV1::from_v3_fixture_state(human_trigger_order_state(true));
    let mut adapter = HumanDecisionProjectorV1::new(PlayerSeatV1::P0);
    assert_eq!(adapter.project_current(&session, decision(&session)), Err(HumanDecisionErrorV1::UnsupportedPrompt));
    assert!(adapter.pending.is_none());
}
