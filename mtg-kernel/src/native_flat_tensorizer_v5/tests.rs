use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::rl_session::FastActorResponseV1;
use crate::state::{GameState, Zone};
use sha2::{Digest, Sha256};

fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("fixture is not a live decision");
    };
    expected
}

fn ordinary_state(actor: PlayerId) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Mountain", Zone::Hand);
    state
}

fn fixture_states() -> Vec<(&'static str, GameState)> {
    let (shared, _) = crate::rl_session::hidden_order_triggers_shared_source_state_v1();
    let (mut single, source, _, _) =
        crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
    crate::rl_session::shuffle_trigger_source_into_library_v1(&mut single, source, PlayerId::P0);
    let mut states = vec![
        ("ordinary_p0", ordinary_state(PlayerId::P0)),
        ("ordinary_p1", ordinary_state(PlayerId::P1)),
    ];
    states.push(("hidden_shared", shared));
    states.push(("hidden_single_target", single));
    states.push(("ward_source_changed_controller", ward_state(true)));
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    {
        use crate::policy_observation_v8::tests::actual_sire_state;
        for (name, actor, life, targeters, pending) in [
            ("actual_sire_visible_p0", PlayerId::P0, 20, 0, false),
            ("actual_sire_visible_p1", PlayerId::P1, 20, 0, false),
            ("actual_sire_pending_exact_p0", PlayerId::P0, 7, 1, true),
            ("actual_sire_pending_exact_p1", PlayerId::P1, 7, 1, true),
            ("actual_sire_queued_multi_p0", PlayerId::P0, 20, 2, false),
            ("actual_sire_queued_multi_p1", PlayerId::P1, 20, 2, false),
        ] {
            states.push((name, actual_sire_state(actor, life, targeters, pending)));
        }
    }
    #[cfg(feature = "standard-magezero-fixtures")]
    {
        for (name, actor) in [
            ("printed_back_p0", PlayerId::P0),
            ("printed_back_p1", PlayerId::P1),
        ] {
            let mut state = ordinary_state(actor);
            let source = put(
                &mut state,
                actor.opponent(),
                "Brutal Cathar",
                Zone::Battlefield,
            );
            state.objects.get_mut(source).v4.face_index = 1;
            state.day_night_v1 = Some(crate::state::DayNightV1::Night);
            put(&mut state, actor, "Brutal Cathar", Zone::Hand);
            put(&mut state, actor.opponent(), "Brutal Cathar", Zone::Hand);
            states.push((name, state));
        }
    }
    states
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn actual_sire_payment_binding_restore_and_refusal_preserve_authority() {
    use crate::policy_observation_v8::tests::actual_sire_state;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session =
            FastActorSessionV1::from_v5_fixture_state(actual_sire_state(actor, 7, 1, true));
        let e = expected(&session);
        let mut encoder = NativeFlatPolicyEncoderV5::default();
        let output = encoder.encode(&session, e).unwrap();
        let (observation, _) = session.flat_rich_current_input_v5(e).unwrap();
        assert_eq!(
            observation
                .extensions
                .pending_ward_life_payment
                .as_ref()
                .unwrap()
                .life,
            7
        );
        assert_eq!(observation.extensions.ward_life_card_traits.len(), 1);
        let before = session.privileged_core_environment_hash();
        let mut forged = output.binding();
        forged.1[0] ^= 1;
        assert!(session.step_flat_action_v5(e, forged, 0).is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        let snapshot = session.snapshot_v1();
        let response = session.step_flat_action_v5(e, output.binding(), 0).unwrap();
        let after = session.privileged_core_environment_hash();
        session.restore_v1(&snapshot);
        assert_eq!(encoder.encode(&session, e).unwrap(), output);
        assert_eq!(
            session.step_flat_action_v5(e, output.binding(), 0).unwrap(),
            response
        );
        assert_eq!(session.privileged_core_environment_hash(), after);
    }
}

fn ward_state(changed_controller: bool) -> GameState {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Target;
    let mut state = ordinary_state(PlayerId::P0);
    let target = put(
        &mut state,
        PlayerId::P1,
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let spell = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    put(&mut state, PlayerId::P1, "Lotus Petal", Zone::Hand);
    put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 8;
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    if changed_controller {
        // Consistent typed-state fixture for a captured source whose live
        // controller differs. This does not claim a control-change spell.
        state.players[1].battlefield.retain(|&id| id != target);
        state.players[0].battlefield.push(target);
        state.objects.get_mut(target).controller = PlayerId::P0;
    }
    state
}

#[test]
fn captured_ward_source_controller_is_independent_of_its_live_alias() {
    let session = FastActorSessionV1::from_v5_fixture_state(ward_state(true));
    let mut encoder = NativeFlatPolicyEncoderV5::default();
    let output = encoder.encode(&session, expected(&session)).unwrap();
    assert!(!output.edge_features().is_empty());
    let (observation, _) = session
        .flat_rich_current_input_v5(expected(&session))
        .unwrap();
    assert_eq!(observation.extensions.queued_ward_payments.len(), 1);
    let history = &observation.extensions.historical_public_sources;
    assert_eq!(history[0].source.controller, crate::rl::PlayerSeatV1::P1);
    assert!(observation.projection.surface.battlefield[0]
        .iter()
        .any(|card| card.stable.arena_id == history[0].source.arena_id
            && card.stable.controller == crate::rl::PlayerSeatV1::P0));
}

fn encoded_json(output: &NativeFlatPolicyDecisionV5) -> serde_json::Value {
    let tensor = &output.tensor.common;
    serde_json::json!({"state": tensor.state, "object_features": tensor.object_features,
        "object_card_ids": tensor.object_card_ids, "object_groups": tensor.object_groups,
        "object_node_ids": tensor.object_node_ids, "edge_features": tensor.edge_features,
        "edge_source_indices": tensor.edge_source_indices, "edge_target_indices": tensor.edge_target_indices,
        "action_features": tensor.action_features, "action_ref_features": tensor.action_ref_features,
        "action_ref_card_ids": tensor.action_ref_card_ids, "action_ref_action_indices": tensor.action_ref_action_indices,
        "action_ref_node_indices": tensor.action_ref_node_indices})
}

fn typed_life_boundary_case(pending: bool) -> serde_json::Value {
    use crate::flat_policy_v5::{FlatQueuedWardLifePaymentV5, FlatWardLifePaymentV5};
    use crate::policy_observation_v8::{QueuedWardLifePaymentV8, WardLifePaymentV8};
    let (mut state, _, source) = crate::policy_observation_v6::tests::ward_multi_targeter_state();
    if pending {
        state.players[0].life = 3;
        crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
    } else {
        state.players[1].battlefield.retain(|&id| id != source);
        state.players[0].battlefield.push(source);
        state.objects.get_mut(source).controller = PlayerId::P0;
    }
    let session = FastActorSessionV1::from_v5_fixture_state(state);
    let e = expected(&session);
    let mut encoder = NativeFlatPolicyEncoderV5::default();
    let original = encoder.encode(&session, e).unwrap();
    let (mut observation, semantics) = session.flat_rich_current_input_v5(e).unwrap();
    let mut buffers = OwnedScoringV5::default();
    let mut flat = FlatDecisionEncoderV5::default();
    let mut decision = session
        .encode_current_flat_scoring_decision_owned_v5(
            e,
            &mut flat,
            &mut FlatScoringOwnedBuffersV2 {
                objects: &mut buffers.objects,
                relations: &mut buffers.relations,
                object_subtypes: &mut buffers.object_subtypes,
                ability_uses: &mut buffers.ability_uses,
                goads: &mut buffers.goads,
                completed_dungeons: &mut buffers.completed_dungeons,
                effect_subtype_changes: &mut buffers.effect_subtype_changes,
                context_path_elements: &mut buffers.context_path_elements,
                actions: &mut buffers.actions,
                action_refs: &mut buffers.action_refs,
            },
        )
        .unwrap();
    // A typed tensor-boundary fixture replaces already authenticated Generic
    // relations. It exercises no life-payment producer, binding or gameplay.
    if let Some(payment) = observation.extensions.pending_ward_payment.take() {
        observation.extensions.pending_ward_life_payment = Some(WardLifePaymentV8 {
            targeting_stack_index: payment.targeting_stack_index,
            payer: payment.payer,
            life: 3,
        });
    }
    for (index, queued) in std::mem::take(&mut observation.extensions.queued_ward_payments)
        .into_iter()
        .enumerate()
    {
        observation
            .extensions
            .queued_ward_life_payments
            .push(QueuedWardLifePaymentV8 {
                stack_index: queued.stack_index,
                payment: WardLifePaymentV8 {
                    targeting_stack_index: queued.payment.targeting_stack_index,
                    payer: queued.payment.payer,
                    life: 3 + index as u8,
                },
            });
    }
    if let Some(payment) = decision.extensions.pending_ward_payment.take() {
        decision.extensions.pending_ward_life_payment = Some(FlatWardLifePaymentV5 {
            targeting_stack_index: payment.targeting_stack_index,
            targeting_source_object: payment.targeting_source_object,
            ward_source_object: payment.ward_source_object,
            payer: payment.payer,
            life: 3,
        });
    }
    for (index, queued) in std::mem::take(&mut decision.extensions.queued_ward_payments)
        .into_iter()
        .enumerate()
    {
        decision
            .extensions
            .queued_ward_life_payments
            .push(FlatQueuedWardLifePaymentV5 {
                stack_index: queued.stack_index,
                payment: FlatWardLifePaymentV5 {
                    targeting_stack_index: queued.payment.targeting_stack_index,
                    targeting_source_object: queued.payment.targeting_source_object,
                    ward_source_object: queued.payment.ward_source_object,
                    payer: queued.payment.payer,
                    life: 3 + index as u8,
                },
            });
    }
    let mut tensor = NativeFlatDecisionTensorV5::default();
    NativeFlatTensorizerV5::default()
        .fill(buffers.view(&decision), &mut tensor)
        .unwrap();
    let typed = NativeFlatPolicyDecisionV5 {
        binding: original.binding(),
        tensor,
    };
    let life_edges = typed
        .edge_features()
        .chunks_exact(41)
        .filter(|edge| edge[15] == 38.0 / 64.0 || edge[15] == 39.0 / 64.0)
        .collect::<Vec<_>>();
    assert_eq!(life_edges.len(), 2);
    assert!(life_edges.iter().all(|edge| edge[17] == 1.0));
    if pending {
        assert!(life_edges.iter().any(|edge| edge[15] == 38.0 / 64.0));
    } else {
        assert_ne!(life_edges[0][14], life_edges[1][14]);
        assert_ne!(life_edges[0][20], life_edges[1][20]);
    }
    let actions = semantics
        .into_iter()
        .enumerate()
        .map(|(index, semantic)| {
            crate::rl::make_legal_action_v5(index as u32, semantic, None).unwrap()
        })
        .collect::<Vec<_>>();
    serde_json::json!({"name": if pending { "typed_boundary_life_pending_exact" } else { "typed_boundary_life_queued_multi_targeter" },
        "observation": observation, "legal_actions": actions, "tensors": encoded_json(&typed)})
}

#[test]
fn life_payment_tensor_boundary_distinguishes_targeters_and_exact_life() {
    typed_life_boundary_case(false);
    typed_life_boundary_case(true);
}

#[test]
fn independent_v5_source_descriptor_model_and_old_loader_refusal() {
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(include_bytes!(
                "../../../python/mtg_kernel_rl/features_v8.py"
            ))
        ),
        FEATURES_SOURCE_SHA256_V5
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(include_bytes!(
                "../../../data/flat_policy_v5/feature_contract_v5.json"
            ))
        ),
        FEATURE_DESCRIPTOR_SHA256_V5
    );
    assert_eq!(MODEL_CONTRACT_VERSION_V5, "kernel-policy-value-net-10");
    assert_ne!(
        FEATURE_CONTRACT_DIGEST_V5,
        crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4
    );
    assert_ne!(
        FEATURE_ENCODING_DIGEST_V5,
        crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4
    );
    let session = FastActorSessionV1::from_v5_fixture_state(ordinary_state(PlayerId::P0));
    let output = NativeFlatPolicyEncoderV5::default()
        .encode(&session, expected(&session))
        .unwrap();
    let model = crate::native_policy_value_net_v1::NativePolicyValueNetV1::runner_fixed_v1(
        crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1::contract_v1(),
    )
    .unwrap();
    let view = encoded_decision_view_v5(&output.tensor);
    assert!(model.forward_v1(view).is_err());
    assert!(model.forward_feature_transfer_v3(view).is_err());
    assert!(model.forward_feature_transfer_v4(view).is_err());
}

#[test]
fn v5_session_is_explicit_binding_is_readonly_and_restore_replays() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let legacy = FastActorSessionV1::from_v3_fixture_state(ordinary_state(actor));
        let e = expected(&legacy);
        let before = legacy.privileged_core_environment_hash();
        let mut encoder = NativeFlatPolicyEncoderV5::default();
        assert!(encoder.encode(&legacy, e).is_err());
        assert_eq!(legacy.privileged_core_environment_hash(), before);
        let mut session = legacy.into_flat_policy_v5().unwrap();
        let output = encoder.encode(&session, e).unwrap();
        let mut forged = output.binding();
        forged.1[0] ^= 1;
        let before = session.privileged_core_environment_hash();
        assert!(session.step_flat_action_v5(e, forged, 0).is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        assert!(session
            .step_flat_action_v5(e, output.binding(), e.legal_action_count)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        let mut stale = e;
        stale.environment_revision += 1;
        assert!(encoder.encode(&session, stale).is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        let snapshot = session.snapshot_v1();
        let response = session.step_flat_action_v5(e, output.binding(), 0).unwrap();
        let after = session.privileged_core_environment_hash();
        session.restore_v1(&snapshot);
        assert_eq!(encoder.encode(&session, e).unwrap(), output);
        assert_eq!(
            session.step_flat_action_v5(e, output.binding(), 0).unwrap(),
            response
        );
        assert_eq!(session.privileged_core_environment_hash(), after);
    }
}

#[test]
fn shared_hidden_trigger_positions_survive_complete_v5_pipeline() {
    let (state, _) = crate::rl_session::hidden_order_triggers_shared_source_state_v1();
    let session = FastActorSessionV1::from_v5_fixture_state(state);
    let mut encoder = NativeFlatPolicyEncoderV5::default();
    let output = encoder.encode(&session, expected(&session)).unwrap();
    let nodes = output.action_ref_node_indices();
    assert_eq!(nodes.len(), 4);
    assert_ne!(nodes[0], nodes[1]);
    assert_eq!(&nodes[..2], &nodes[2..]);
    assert_eq!(
        output,
        encoder.encode(&session, expected(&session)).unwrap()
    );
}

#[cfg(feature = "standard-magezero-fixtures")]
#[test]
fn printed_trait_shared_hidden_source_reaches_each_alias_without_new_identity() {
    let (mut state, source) = crate::rl_session::hidden_order_triggers_shared_source_state_v1();
    let definition = crate::card_def::card_id_by_name("Brutal Cathar").unwrap();
    state.objects.get_mut(source).card_def = definition;
    // Typed projection fixture: preserve each captured position but replace
    // its immutable printed definition. This does not test Cathar triggers.
    for trigger in &mut state.engine.pending_triggers {
        trigger.source_contract.as_mut().unwrap().card_def = definition;
    }
    let session = FastActorSessionV1::from_v5_fixture_state(state);
    let mut encoder = NativeFlatPolicyEncoderV5::default();
    let output = encoder.encode(&session, expected(&session)).unwrap();
    let nodes = output.action_ref_node_indices();
    assert_ne!(nodes[0], nodes[1]);
    let width = 41;
    let edges = output
        .edge_features()
        .chunks_exact(width)
        .enumerate()
        .filter_map(|(index, edge)| (edge[15] == 40.0 / 64.0).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), 2);
    assert_eq!(
        edges
            .iter()
            .map(|&index| output.edge_source_indices()[index])
            .collect::<std::collections::BTreeSet<_>>(),
        nodes[..2].iter().copied().collect()
    );
}

#[test]
fn v5_parity_fixture_export_from_actual_registered_states() {
    let mut cases = Vec::new();
    for (name, state) in fixture_states() {
        let session = FastActorSessionV1::from_v5_fixture_state(state);
        let e = expected(&session);
        let before = session.privileged_core_environment_hash();
        let output = NativeFlatPolicyEncoderV5::default()
            .encode(&session, e)
            .unwrap();
        let (observation, semantics) = session.flat_rich_current_input_v5(e).unwrap();
        let actions = semantics
            .into_iter()
            .enumerate()
            .map(|(index, semantic)| {
                crate::rl::make_legal_action_v5(index as u32, semantic, None).unwrap()
            })
            .collect::<Vec<_>>();
        cases.push(serde_json::json!({"name": name, "observation": observation,
            "legal_actions": actions, "tensors": encoded_json(&output)}));
        assert_eq!(session.privileged_core_environment_hash(), before);
    }
    cases.push(typed_life_boundary_case(false));
    cases.push(typed_life_boundary_case(true));
    if let Ok(path) = std::env::var("MTG_FLAT_V5_PARITY_EXPORT") {
        use std::io::Write;
        let payload = serde_json::json!({"schema": "flat-v5-native-python-parity-1",
            "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V5,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V5, "cases": cases});
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap();
        file.write_all(&serde_json::to_vec(&payload).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }
}
