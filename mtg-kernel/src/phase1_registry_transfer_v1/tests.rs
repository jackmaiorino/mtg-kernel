use super::*;
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
};
use crate::sideboard_play_policy_v1::{
    FrozenPlayObservationReceiptV3, FrozenPlayObservationTransferV3, FrozenPlayPolicyV1,
};

const LEARNING_RATE: f32 = 0.00003;
const VALUE_COEFFICIENT: f32 = 0.75;

// Real native forward/backward/Adam on a small, declared synthetic V3 decision.
// This verifies state continuation, not legal-game behavior or playing strength.
fn update(state: &mut NativePolicyValueTrainStateV1, token: i64) {
    let mut features = vec![0.0; 390];
    features[0] = 0.25;
    features[195] = -0.5;
    let tensor = NativeFlatDecisionTensorV3 {
        common: NativeFlatDecisionTensorV2 {
            state: vec![0.125; 219],
            object_features: vec![0.0625; 98],
            object_card_ids: vec![token],
            object_groups: vec![0],
            object_node_ids: vec![0],
            action_features: features,
            ..Default::default()
        },
    };
    let output = state
        .model_v1()
        .forward_feature_transfer_v3(encoded_decision_view_v3(&tensor))
        .unwrap();
    let logits: Vec<u32> = output.logits.iter().map(|x| x.to_bits()).collect();
    let steps = [NativePolicySubstepV1 {
        forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(&tensor))),
        selected_action_index: 1,
        expected_raw_action_logit_bits: &logits,
        expected_value_bits: output.value.to_bits(),
    }];
    state
        .train_step_feature_transfer_v3(
            &[NativePolicyPhysicalDecisionV1 {
                substeps: &steps,
                terminal_return: 1,
                baseline_bits: 0,
            }],
            VALUE_COEFFICIENT,
            LEARNING_RATE,
        )
        .unwrap();
}

fn learned_state() -> NativePolicyValueTrainStateV1 {
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let mut state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
    update(&mut state, 1);
    update(&mut state, 1);
    let snapshot = state.snapshot_v1().unwrap();
    assert!(snapshot
        .first_moments
        .iter()
        .flat_map(|p| &p.values)
        .any(|v| *v != 0.0));
    assert!(snapshot
        .second_moments
        .iter()
        .flat_map(|p| &p.values)
        .any(|v| *v > 0.0));
    state
}

fn source_registry(count: usize) -> Vec<u8> {
    let mut registry: Value = serde_json::from_slice(CURRENT_REGISTRY).unwrap();
    registry["cards"].as_array_mut().unwrap().truncate(count);
    serde_json::to_vec(&registry).unwrap()
}

fn checkpoint(
    snapshot: &NativePolicyValueTrainSnapshotV1,
    registry: &[u8],
) -> ExpandedCheckpointInputV1 {
    let count = registry_cards(registry).unwrap().len();
    let mut ancestry = FrozenPlayPolicyV1::training_fixture_v3()
        .identity_v1()
        .as_imported_v1()
        .unwrap()
        .clone();
    ancestry.destination_registry_sha256 = sha(registry);
    ancestry.destination_card_count = count;
    ancestry.source_card_count = count;
    ancestry.destination_card_db_hash = "0123456789abcdef".into();
    ancestry.observation_successor = Some(FrozenPlayObservationReceiptV3 {
        schema: "mtg-kernel-frozen-play-observation-transfer/v3".into(),
        source_feature_contract_digest: "1".repeat(64),
        source_feature_encoding_digest: "2".repeat(64),
        destination: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
        },
        features_source_sha256: FEATURES_SOURCE_SHA256_V3.into(),
        feature_descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V3.into(),
        semantics: "synthetic test provenance, never an evidence checkpoint".into(),
    });
    ExpandedCheckpointInputV1 {
        schema: "mtg-kernel-expanded-deck-checkpoint/v1".into(),
        feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
        feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
        card_db_hash: ancestry.destination_card_db_hash.clone(),
        source_import: ancestry,
        state_sha256: hex_digest(&snapshot.state_sha256_v1().unwrap()),
        adam_step: snapshot.adam_step,
        scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
        parameters: tensor_bits(&snapshot.parameters),
        first_moments: tensor_bits(&snapshot.first_moments),
        second_moments: tensor_bits(&snapshot.second_moments),
        trajectories: vec![PinnedFileV1 {
            path: "preserved-test-trajectory.json".into(),
            sha256: "3".repeat(64),
        }],
        loss_identity: LOSS.into(),
        gamma_bits: None,
        gae_lambda_bits: None,
        entropy_coefficient_bits: None,
        learning_rate_bits: LEARNING_RATE.to_bits(),
        value_coefficient_bits: VALUE_COEFFICIENT.to_bits(),
    }
}

fn request(
    saved: &ExpandedCheckpointInputV1,
    bytes: &[u8],
    registry: &[u8],
) -> RegistryTransferRequestV1 {
    RegistryTransferRequestV1 {
        source_checkpoint_sha256: sha(bytes),
        source_registry_sha256: sha(registry),
        source_state_sha256: saved.state_sha256.clone(),
        source_adam_step: saved.adam_step,
        source_card_db_hash: saved.card_db_hash.clone(),
        destination_registry_sha256: sha(CURRENT_REGISTRY),
        destination_card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        features: RegistryTransferFeaturesV1::current_v1(),
        initialization_seed: 123456,
    }
}

fn transfer(
    snapshot: &NativePolicyValueTrainSnapshotV1,
    source_count: usize,
) -> VerifiedRegistryTransferV1 {
    let registry = source_registry(source_count);
    let saved = checkpoint(snapshot, &registry);
    let bytes = serde_json::to_vec(&saved).unwrap();
    transfer_expanded_checkpoint_to_current_registry_v1(
        &bytes,
        &registry,
        &request(&saved, &bytes, &registry),
    )
    .unwrap()
}

fn assert_shared_exact(
    before: &NativePolicyValueTrainSnapshotV1,
    after: &NativePolicyValueTrainSnapshotV1,
    source_count: usize,
) {
    assert_eq!(before.adam_step, after.adam_step);
    assert_eq!(
        before.scorer_bias_anchor_bits,
        after.scorer_bias_anchor_bits
    );
    let first_new = (source_count + 1) * CARD_EMBEDDING_DIM_V1;
    let past_new = (CARD_DEFS.len() + 1) * CARD_EMBEDDING_DIM_V1;
    for (old, new) in [
        (&before.parameters, &after.parameters),
        (&before.first_moments, &after.first_moments),
        (&before.second_moments, &after.second_moments),
    ] {
        for (ordinal, (a, b)) in old.iter().zip(new).enumerate() {
            assert_eq!((a.name, &a.shape), (b.name, &b.shape));
            for (index, (x, y)) in a.values.iter().zip(&b.values).enumerate() {
                if ordinal == 0 && (first_new..past_new).contains(&index) {
                    continue;
                }
                assert_eq!(x.to_bits(), y.to_bits(), "{}[{index}] changed", a.name);
            }
        }
    }
}

#[test]
fn phase1_registry_identity_preserves_nondefault_gauge_and_exact_adam_continuation() {
    let mut snapshot = learned_state().snapshot_v1().unwrap();
    let gauge = snapshot
        .parameters
        .iter_mut()
        .find(|p| p.name == "scorer.2.bias")
        .unwrap();
    gauge.values[0] = 0.3125;
    snapshot.scorer_bias_anchor_bits = 0.3125_f32.to_bits();
    let mut control = restore(&snapshot).unwrap();
    let candidate = transfer(&snapshot, CARD_DEFS.len());
    assert_eq!(snapshot, candidate.state.snapshot_v1().unwrap());
    assert_eq!(candidate.receipt_v1().initialized_embedding_scalar_count, 0);
    let (mut resumed, settings) = candidate.into_train_state_v1();
    assert_eq!(settings.learning_rate_bits, LEARNING_RATE.to_bits());
    assert_eq!(settings.value_coefficient_bits, VALUE_COEFFICIENT.to_bits());
    assert_eq!(settings.adam_beta1_bits, ADAM_BETA1_V1.to_bits());
    assert_eq!(settings.adam_beta2_bits, ADAM_BETA2_V1.to_bits());
    assert_eq!(settings.adam_epsilon_bits, ADAM_EPSILON_V1.to_bits());
    assert_eq!(
        settings.adam_weight_decay_bits,
        ADAM_WEIGHT_DECAY_V1.to_bits()
    );
    update(&mut control, 1);
    update(&mut resumed, 1);
    assert_eq!(
        control.snapshot_v1().unwrap(),
        resumed.snapshot_v1().unwrap()
    );
    assert_eq!(resumed.adam_step_v1(), 3);
}

#[test]
fn phase1_registry_append_preserves_all_shared_bits_and_existing_card_next_update() {
    let mut control = learned_state();
    let before = control.snapshot_v1().unwrap();
    let source_count = CARD_DEFS.len() - 2;
    let candidate = transfer(&before, source_count);
    let after = candidate.state.snapshot_v1().unwrap();
    assert_shared_exact(&before, &after, source_count);
    assert_eq!(
        candidate.receipt_v1().initialized_embedding_scalar_count,
        32
    );
    for id in source_count..CARD_DEFS.len() {
        let row = (id + 1) * CARD_EMBEDDING_DIM_V1..(id + 2) * CARD_EMBEDDING_DIM_V1;
        assert_eq!(
            &after.parameters[0].values[row.clone()],
            &initial_card_row(CARD_DEFS[id].name, 123456)
        );
        assert!(after.first_moments[0].values[row.clone()]
            .iter()
            .all(|v| v.to_bits() == 0));
        assert!(after.second_moments[0].values[row]
            .iter()
            .all(|v| v.to_bits() == 0));
    }
    let (mut resumed, _) = candidate.into_train_state_v1();
    update(&mut control, 1);
    update(&mut resumed, 1);
    assert_shared_exact(
        &control.snapshot_v1().unwrap(),
        &resumed.snapshot_v1().unwrap(),
        source_count,
    );
    assert_eq!(resumed.adam_step_v1(), 3);
}

#[test]
fn phase1_registry_new_card_row_learns_after_transfer_without_resetting_step() {
    let before = learned_state().snapshot_v1().unwrap();
    let candidate = transfer(&before, CARD_DEFS.len() - 1);
    let initial = candidate.state.snapshot_v1().unwrap();
    let (mut state, _) = candidate.into_train_state_v1();
    update(&mut state, CARD_DEFS.len() as i64);
    let after = state.snapshot_v1().unwrap();
    let row =
        CARD_DEFS.len() * CARD_EMBEDDING_DIM_V1..(CARD_DEFS.len() + 1) * CARD_EMBEDDING_DIM_V1;
    assert_ne!(
        initial.parameters[0].values[row.clone()],
        after.parameters[0].values[row.clone()]
    );
    assert!(after.first_moments[0].values[row.clone()]
        .iter()
        .any(|v| *v != 0.0));
    assert!(after.second_moments[0].values[row].iter().any(|v| *v > 0.0));
    assert_eq!(after.adam_step, before.adam_step + 1);
}

#[test]
fn phase1_registry_rejects_removal_reorder_definition_changes_and_duplicate_names() {
    let cards = registry_cards(CURRENT_REGISTRY).unwrap();
    assert!(mapping(&cards, &cards[..cards.len() - 1]).is_err());
    let mut changed = cards.clone();
    changed.swap(0, 1);
    assert!(mapping(&cards, &changed).is_err());
    let mut changed = cards.clone();
    changed[0]["mana_value"] = serde_json::json!(99);
    assert!(mapping(&cards, &changed).is_err());
    let mut duplicate: Value = serde_json::from_slice(CURRENT_REGISTRY).unwrap();
    duplicate["cards"][1]["name"] = duplicate["cards"][0]["name"].clone();
    assert!(registry_cards(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    assert!(registry_cards(br#"{"cards":[{"name":"A","name":"B"}]}"#).is_err());
}

#[test]
fn phase1_registry_rejects_feature_changes_wrong_pins_and_malformed_state() {
    let snapshot = learned_state().snapshot_v1().unwrap();
    let registry = source_registry(CARD_DEFS.len());
    let mut saved = checkpoint(&snapshot, &registry);
    let bytes = serde_json::to_vec(&saved).unwrap();
    let original = request(&saved, &bytes, &registry);
    let mut changed = original.clone();
    changed.features.encoding_digest = "0".repeat(64);
    assert!(
        transfer_expanded_checkpoint_to_current_registry_v1(&bytes, &registry, &changed).is_err()
    );
    let mut changed = original.clone();
    changed.source_adam_step += 1;
    assert!(
        transfer_expanded_checkpoint_to_current_registry_v1(&bytes, &registry, &changed).is_err()
    );
    let mut changed = original;
    changed.source_registry_sha256 = "0".repeat(64);
    assert!(
        transfer_expanded_checkpoint_to_current_registry_v1(&bytes, &registry, &changed).is_err()
    );
    saved.second_moments[1].values[0] = (-0.1_f32).to_bits();
    let corrupt = serde_json::to_vec(&saved).unwrap();
    assert!(transfer_expanded_checkpoint_to_current_registry_v1(
        &corrupt,
        &registry,
        &request(&saved, &corrupt, &registry)
    )
    .is_err());
}

#[test]
fn phase1_registry_refuses_to_erase_preexisting_learning_in_new_card_row() {
    let mut state = learned_state();
    update(&mut state, CARD_DEFS.len() as i64);
    let registry = source_registry(CARD_DEFS.len() - 1);
    let saved = checkpoint(&state.snapshot_v1().unwrap(), &registry);
    let bytes = serde_json::to_vec(&saved).unwrap();
    let error = transfer_expanded_checkpoint_to_current_registry_v1(
        &bytes,
        &registry,
        &request(&saved, &bytes, &registry),
    )
    .err()
    .unwrap();
    assert!(error.contains("learned optimizer state"), "{error}");
}

#[test]
fn phase1_registry_artifact_replay_rejects_tampered_state_even_with_new_artifact_digest() {
    let snapshot = learned_state().snapshot_v1().unwrap();
    let registry = source_registry(CARD_DEFS.len() - 1);
    let saved = checkpoint(&snapshot, &registry);
    let bytes = serde_json::to_vec(&saved).unwrap();
    let candidate = transfer_expanded_checkpoint_to_current_registry_v1(
        &bytes,
        &registry,
        &request(&saved, &bytes, &registry),
    )
    .unwrap();
    let artifact = candidate.artifact_bytes_v1().unwrap();
    let reloaded =
        verify_registry_transfer_artifact_v1(&artifact, &sha(&artifact), &bytes, &registry)
            .unwrap();
    assert_eq!(
        candidate.state.snapshot_v1().unwrap(),
        reloaded.state.snapshot_v1().unwrap()
    );
    assert_eq!(artifact, reloaded.artifact_bytes_v1().unwrap());
    let mut envelope: TransferEnvelopeV1 = serde_json::from_slice(&artifact).unwrap();
    envelope.first_moments[1].values[0] ^= 1;
    let tampered = serde_json::to_vec(&envelope).unwrap();
    assert!(
        verify_registry_transfer_artifact_v1(&tampered, &sha(&tampered), &bytes, &registry)
            .is_err()
    );
}

#[test]
fn phase1_registry_initializer_is_identity_seed_bound_and_uses_no_padding_row() {
    let row = initial_card_row("Lightning Bolt", 7);
    assert_eq!(row, initial_card_row("Lightning Bolt", 7));
    assert_ne!(row, initial_card_row("Lightning Bolt", 8));
    assert_ne!(row, initial_card_row("Lava Spike", 7));
    assert!(row
        .iter()
        .all(|v| v.is_finite() && (-0.03125..0.03125).contains(v)));
    let cards = registry_cards(CURRENT_REGISTRY).unwrap();
    let mapped = mapping(&cards[..cards.len() - 1], &cards).unwrap();
    assert!(mapped.iter().all(|card| card.destination_embedding_row > 0));
}
