use super::*;
use crate::bo3_match::PlayDrawChoiceV1;
use crate::native_flat_tensorizer_v3::{NativeFlatDecisionTensorV3, encoded_decision_view_v3};
use crate::native_flat_tensorizer_v4::{NativeFlatDecisionTensorV4, encoded_decision_view_v4};
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
};
use crate::native_policy_value_net_v1::{NativePolicyValueModelConfigV1, NativePolicyValueNetV1};
use crate::paired_bo1_harness_v1::PairedBo1PolicyV1;
use crate::phase1_bo3_collection_v1::tests::{config, fixtures, fixtures_v4};
use crate::phase1_bo3_learning_v1::{
    BO3_PREPARATION_REQUEST_SCHEMA_V1, Bo3AttemptInputV1, Bo3PreparationLimitsV1,
};

fn native_state_and_tensor() -> (NativePolicyValueTrainStateV1, NativeFlatDecisionTensorV3) {
    use crate::human_opening_v1::HumanOpeningV1;
    use crate::ids::PlayerId;
    use crate::paired_bo1_harness_v1::{PairedBo1PolicyInputV1, paired_policy_seeds_v1};
    use crate::rl_session::FastActorResponseV1;
    let (mut policies, _) = fixtures([PlayDrawChoiceV1::Play; 2]);
    let cfg = config("continuation-native-tensor");
    let mut opening = HumanOpeningV1::new(
        1,
        117,
        100,
        100,
        cfg.deck_ids,
        cfg.registrations.each_ref().map(|d| d.mainboard.clone()),
        PlayerId::P0,
        PlayerId::P0,
    )
    .unwrap();
    opening.keep().unwrap();
    let session = opening.into_session().unwrap();
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real initial decision missing")
    };
    policies[0]
        .reset_for_game_v1(paired_policy_seeds_v1(117))
        .unwrap();
    let input = PairedBo1PolicyInputV1::new(&session, decision);
    policies[0].select_paired_with_scores_v1(&input).unwrap();
    let tensor = policies[0]
        .last_scored_training_tensor_v3()
        .unwrap()
        .clone();
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    model
        .replace_parameter_snapshot_v1(&policies[0].training_parameters_v3())
        .unwrap();
    let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
    let mut snapshot = state.snapshot_v1().unwrap();
    snapshot.adam_step = 7;
    snapshot
        .first_moments
        .iter_mut()
        .find(|p| p.name == "value_head.2.bias")
        .unwrap()
        .values[0] = 0.015;
    snapshot
        .second_moments
        .iter_mut()
        .find(|p| p.name == "value_head.2.bias")
        .unwrap()
        .values[0] = 0.023;
    (
        NativePolicyValueTrainStateV1::from_snapshot_v1(state.model_v1().clone(), &snapshot)
            .unwrap(),
        tensor,
    )
}
fn numerical_update(
    state: &mut NativePolicyValueTrainStateV1,
    tensor: &NativeFlatDecisionTensorV3,
) {
    // Numerical continuation fixture using a real original V3 shape. Re-score
    // at the current parameters before each update; this is not a new match.
    let output = state
        .model_v1()
        .forward_feature_transfer_v3(encoded_decision_view_v3(tensor))
        .unwrap();
    let logits: Vec<_> = output.logits.iter().map(|v| v.to_bits()).collect();
    let steps = [NativePolicySubstepV1 {
        forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(tensor))),
        selected_action_index: 0,
        expected_raw_action_logit_bits: &logits,
        expected_value_bits: output.value.to_bits(),
    }];
    let groups = [NativePolicyPhysicalDecisionV1 {
        substeps: &steps,
        terminal_return: 1,
        baseline_bits: 0,
    }];
    state
        .train_step_weighted_feature_transfer_v3(&groups, &[1.0], 0.5, 0.00001)
        .unwrap();
}

/// V4 sibling of `native_state_and_tensor`, sourced from the fresh-lineage
/// V4 fixture instead of V3's. `apply_prepared` (the update entry point's
/// caller of `with_native_groups_v1`) dispatches to
/// `train_step_weighted_feature_transfer_v4` for exactly this generation;
/// this fixture exercises that same numeric/optimizer primitive directly,
/// since `PreparedBo3GameplayBatchV1` and `apply_prepared` themselves are
/// private to two different sibling modules and cannot both be reached from
/// one synthetic test without a file-orchestrated `update_bo3_gameplay_v1`
/// run, which (like the existing V3 real-pinned test below) needs a real
/// root-supplied producer artifact for `verify_producer` to accept.
fn native_state_and_tensor_v4() -> (NativePolicyValueTrainStateV1, NativeFlatDecisionTensorV4) {
    use crate::human_opening_v1::HumanOpeningV1;
    use crate::ids::PlayerId;
    use crate::paired_bo1_harness_v1::{PairedBo1PolicyInputV1, paired_policy_seeds_v1};
    use crate::rl_session::FastActorResponseV1;
    let (mut policies, _) = fixtures_v4([PlayDrawChoiceV1::Play; 2]);
    let cfg = config("continuation-native-tensor-v4");
    let mut opening = HumanOpeningV1::new(
        1,
        117,
        100,
        100,
        cfg.deck_ids,
        cfg.registrations.each_ref().map(|d| d.mainboard.clone()),
        PlayerId::P0,
        PlayerId::P0,
    )
    .unwrap();
    opening.keep().unwrap();
    let session = opening.into_session().unwrap();
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real initial decision missing")
    };
    policies[0]
        .reset_for_game_v1(paired_policy_seeds_v1(117))
        .unwrap();
    let input = PairedBo1PolicyInputV1::new(&session, decision);
    policies[0].select_paired_with_scores_v1(&input).unwrap();
    let tensor = policies[0]
        .last_scored_training_tensor_v4()
        .unwrap()
        .clone();
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    model
        .replace_parameter_snapshot_v1(&policies[0].training_parameters_v3())
        .unwrap();
    let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
    let mut snapshot = state.snapshot_v1().unwrap();
    snapshot.adam_step = 7;
    snapshot
        .first_moments
        .iter_mut()
        .find(|p| p.name == "value_head.2.bias")
        .unwrap()
        .values[0] = 0.015;
    snapshot
        .second_moments
        .iter_mut()
        .find(|p| p.name == "value_head.2.bias")
        .unwrap()
        .values[0] = 0.023;
    (
        NativePolicyValueTrainStateV1::from_snapshot_v1(state.model_v1().clone(), &snapshot)
            .unwrap(),
        tensor,
    )
}
fn numerical_update_v4(
    state: &mut NativePolicyValueTrainStateV1,
    tensor: &NativeFlatDecisionTensorV4,
) {
    // V4 sibling of `numerical_update`: identical arithmetic, only the
    // feature-transfer config (schema validation) and encoded-view builder
    // differ, exactly as `train_step_feature_transfer_v3`/`_v4` already do.
    let output = state
        .model_v1()
        .forward_feature_transfer_v4(encoded_decision_view_v4(tensor))
        .unwrap();
    let logits: Vec<_> = output.logits.iter().map(|v| v.to_bits()).collect();
    let steps = [NativePolicySubstepV1 {
        forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v4(tensor))),
        selected_action_index: 0,
        expected_raw_action_logit_bits: &logits,
        expected_value_bits: output.value.to_bits(),
    }];
    let groups = [NativePolicyPhysicalDecisionV1 {
        substeps: &steps,
        terminal_return: 1,
        baseline_bits: 0,
    }];
    state
        .train_step_weighted_feature_transfer_v4(&groups, &[1.0], 0.5, 0.00001)
        .unwrap();
}

#[test]
fn bo3_continuation_native_full_state_bits_and_fresh_numeric_next_update_survive_reload() {
    let (mut state, tensor) = native_state_and_tensor();
    let origin = state.clone();
    numerical_update(&mut state, &tensor);
    let saved = StateBits::capture(&state).unwrap();
    let decoded: StateBits = strict_json(&json(&saved, MAX_CHECKPOINT_BYTES).unwrap()).unwrap();
    let mut restored = decoded.restore(&origin).unwrap();
    assert_eq!(StateBits::capture(&restored).unwrap(), saved);
    assert_eq!(restored.adam_step_v1(), 8);
    numerical_update(&mut state, &tensor);
    numerical_update(&mut restored, &tensor);
    assert_eq!(
        StateBits::capture(&state).unwrap(),
        StateBits::capture(&restored).unwrap()
    );
    assert_eq!(restored.adam_step_v1(), 9);
    assert_eq!(StateBits::capture(&origin).unwrap().adam_step, 7);
}

/// V4 sibling of the test above: the same checkpoint save/reload plus
/// fresh-numeric-next-update proof, but through
/// `train_step_weighted_feature_transfer_v4`, the exact primitive
/// `apply_prepared` dispatches to for a V4-generation prepared batch on the
/// update entry point's own path.
#[test]
fn bo3_continuation_native_full_state_bits_and_fresh_numeric_next_update_survive_reload_v4() {
    let (mut state, tensor) = native_state_and_tensor_v4();
    let origin = state.clone();
    numerical_update_v4(&mut state, &tensor);
    let saved = StateBits::capture(&state).unwrap();
    let decoded: StateBits = strict_json(&json(&saved, MAX_CHECKPOINT_BYTES).unwrap()).unwrap();
    let mut restored = decoded.restore(&origin).unwrap();
    assert_eq!(StateBits::capture(&restored).unwrap(), saved);
    assert_eq!(restored.adam_step_v1(), 8);
    numerical_update_v4(&mut state, &tensor);
    numerical_update_v4(&mut restored, &tensor);
    assert_eq!(
        StateBits::capture(&state).unwrap(),
        StateBits::capture(&restored).unwrap()
    );
    assert_eq!(restored.adam_step_v1(), 9);
    assert_eq!(StateBits::capture(&origin).unwrap().adam_step, 7);
}

#[test]
fn bo3_continuation_state_rejects_layout_moment_step_gauge_and_parameter_tampering() {
    let (state, _) = native_state_and_tensor();
    let saved = StateBits::capture(&state).unwrap();
    for mutation in 0..6 {
        let mut altered = saved.clone();
        match mutation {
            0 => altered.parameters.swap(0, 1),
            1 => altered.first_moments.last_mut().unwrap().values[0] ^= 1,
            2 => altered.second_moments.last_mut().unwrap().values[0] = (-1.0_f32).to_bits(),
            3 => altered.adam_step += 1,
            4 => altered.scorer_bias_anchor_bits ^= 1,
            _ => altered.parameters[0].values[0] ^= 1,
        }
        assert!(altered.restore(&state).is_err(), "mutation {mutation}");
    }
    assert_eq!(StateBits::capture(&state).unwrap(), saved);
}

fn pin(label: &str, n: u64) -> PinnedFileV1 {
    PinnedFileV1 {
        path: std::env::temp_dir().join(label),
        sha256: format!("{n:064x}"),
    }
}
fn accounting_fixture() -> (
    Bo3GameplayUpdateRequestV1,
    Bo3GameplayPreparationRequestV1,
    AttemptProgress,
) {
    let (_, packages) = fixtures([PlayDrawChoiceV1::Play; 2]);
    let learner = packages[0].gameplay.clone();
    let request = Bo3GameplayUpdateRequestV1 {
        schema: BO3_GAMEPLAY_UPDATE_SCHEMA_V1.into(),
        input: Bo3GameplayUpdateInputV1::OrdinaryCheckpointTransition {
            learner: learner.clone(),
        },
        preparation_request: pin("preparation.json", 10),
        learning_rate_bits: 925353388,
        value_coefficient_bits: 1056964608,
        previous_progress: None,
        output_directory: std::env::temp_dir().join("structural-only-bo3-progress"),
    };
    let preparation = Bo3GameplayPreparationRequestV1 {
        schema: BO3_PREPARATION_REQUEST_SCHEMA_V1.into(),
        learner,
        attempts: vec![Bo3AttemptInputV1 {
            request: pin("capture-request.json", 1),
            result: pin("capture-result.json", 2),
            learner_seat: PlayerSeatV1::P0,
        }],
        limits: Bo3PreparationLimitsV1 {
            max_input_bytes: 512 * 1024 * 1024,
            max_prepared_payload_bytes: 256 * 1024 * 1024,
        },
    };
    let plan = AttemptProgress {
        attempted_batches: 1,
        ledger: vec![[
            format!("{:064x}", 1),
            format!("{:064x}", 2),
            format!("{:064x}", 3),
        ]],
        preparation: PreparationReport {
            disposition: Disposition::NoUpdate,
            attempts: vec![AttemptReport {
                match_id: "accounting-only-incomplete".into(),
                deck_ids: ["a".into(), "b".into()],
                learner_seat: PlayerSeatV1::P0,
                complete: false,
                eligible: false,
                exclusion_reason: Some("decision cap".into()),
                physical_learner_groups: 0,
                captured_substeps: 7,
            }],
            eligible_matches: 0,
            complete_matches: 0,
            incomplete_matches: 1,
            prepared_payload_bytes: 0,
            learner_groups: 0,
            learner_substeps: 0,
            weight_bits: vec![],
            claim: "structural accounting fixture, not an engine result".into(),
        },
    };
    (request, preparation, plan)
}

#[test]
fn bo3_continuation_incomplete_attempts_advance_only_ledger_for_both_input_kinds() {
    let (request, preparation, plan) = accounting_fixture();
    validate_progress(&plan, &request, &preparation, None).unwrap();
    for bo3_parent in [false, true] {
        let count = if bo3_parent { 2 } else { 0 };
        let previous = Progress {
            schema: PROGRESS_SCHEMA.into(),
            operation_request: pin("request.json", 9),
            request: request.clone(),
            ordinary_origin: preparation.learner.clone(),
            completed_bo3_updates: count,
            attempt_progress: plan.clone(),
            result: preparation.learner.clone(),
        };
        let mut next_request = request.clone();
        if bo3_parent {
            next_request.input = Bo3GameplayUpdateInputV1::Bo3Checkpoint {
                learner: preparation.learner.clone(),
            };
        }
        next_request.previous_progress = Some(pin("progress.json", 11));
        let mut next_input = preparation.clone();
        next_input.attempts[0].request.sha256 = format!("{:064x}", 4);
        next_input.attempts[0].result.sha256 = format!("{:064x}", 5);
        let mut next_plan = plan.clone();
        next_plan.attempted_batches = 2;
        next_plan.ledger.push([
            format!("{:064x}", 4),
            format!("{:064x}", 5),
            format!("{:064x}", 6),
        ]);
        validate_progress(&next_plan, &next_request, &next_input, Some(&previous)).unwrap();
        assert_eq!(previous.completed_bo3_updates, count);
        assert_eq!(previous.result, preparation.learner);
        // Changing artifact bytes/labels cannot reuse the old incomplete
        // physical sample under an unchanged learner after NoUpdate.
        next_plan.ledger[1][2] = next_plan.ledger[0][2].clone();
        assert!(
            validate_progress(&next_plan, &next_request, &next_input, Some(&previous)).is_err()
        );
        next_plan.ledger[1][2] = format!("{:064x}", 6);
        next_plan.ledger[0][0] = format!("{:064x}", 99);
        assert!(
            validate_progress(&next_plan, &next_request, &next_input, Some(&previous)).is_err()
        );
    }
}

#[test]
fn bo3_continuation_compact_ledger_covers_planned_window_and_rejects_exhaustion() {
    let mut ledger: Vec<[String; 3]> = (0..MAX_ATTEMPTS)
        .map(|n| {
            [
                format!("{n:064x}"),
                format!("{n:064x}"),
                format!("{n:064x}"),
            ]
        })
        .collect();
    validate_ledger(&ledger).unwrap();
    let bytes = json(&ledger, MAX_PROGRESS_BYTES).unwrap();
    assert!(bytes.len() < 14 * 1024 * 1024);
    ledger.push(std::array::from_fn(|_| format!("{:064x}", MAX_ATTEMPTS)));
    assert!(validate_ledger(&ledger).unwrap_err().contains("exhausted"));
}

#[test]
fn bo3_continuation_recorded_weights_reject_tampering_before_large_allocation() {
    let (_, input, mut plan) = accounting_fixture();
    plan.preparation.disposition = Disposition::Ready;
    plan.preparation.attempts[0].complete = true;
    plan.preparation.attempts[0].eligible = true;
    plan.preparation.attempts[0].exclusion_reason = None;
    plan.preparation.attempts[0].physical_learner_groups = 2;
    plan.preparation.eligible_matches = 1;
    plan.preparation.complete_matches = 1;
    plan.preparation.incomplete_matches = 0;
    plan.preparation.learner_groups = 2;
    plan.preparation.learner_substeps = 3;
    plan.preparation.weight_bits = vec![0.5_f32.to_bits(); 2];
    validate_report(&plan.preparation, &input).unwrap();
    plan.preparation.weight_bits[0] ^= 1;
    assert!(validate_report(&plan.preparation, &input).is_err());
    plan.preparation.attempts[0].physical_learner_groups = usize::MAX;
    assert!(validate_report(&plan.preparation, &input).is_err());
}

#[test]
fn bo3_continuation_public_parser_is_bounded_and_preserves_explicit_transition_tag() {
    let (request, _, _) = accounting_fixture();
    let bytes = json(&request, MAX_REQUEST_BYTES).unwrap();
    assert_eq!(
        Bo3GameplayUpdateRequestV1::from_json_v1(std::str::from_utf8(&bytes).unwrap()).unwrap(),
        request
    );
    assert!(
        Bo3GameplayUpdateRequestV1::from_json_v1(
            r#"{"input":{"kind":"bo3_checkpoint","kind":"ordinary_checkpoint_transition"}}"#
        )
        .is_err()
    );
    assert!(
        Bo3GameplayUpdateRequestV1::from_json_v1(&" ".repeat(MAX_REQUEST_BYTES as usize + 1))
            .is_err()
    );
    assert!(
        update_bo3_gameplay_v1(request).is_err(),
        "unverified fixture metadata cannot enter the public ordinary reader"
    );
}

#[test]
fn bo3_continuation_progress_orphan_recovery_preserves_debris_and_rejects_corrupt_final() {
    let (mut request, preparation, plan) = accounting_fixture();
    let directory = std::env::temp_dir().join(format!(
        "bo3-progress-publication-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir(&directory).unwrap();
    request.output_directory = directory.clone();
    let progress = Progress {
        schema: PROGRESS_SCHEMA.into(),
        operation_request: pin_for(
            directory.join("request.json"),
            &json(&request, MAX_REQUEST_BYTES).unwrap(),
        ),
        request,
        ordinary_origin: preparation.learner.clone(),
        completed_bo3_updates: 0,
        attempt_progress: plan,
        result: preparation.learner,
    };
    // A publication-only fixture. It is never admitted by the native updater.
    let orphan = directory.join(".progress.json.stage");
    let retry_orphan = directory.join(".progress.json.stage-recovery-000000");
    std::fs::write(&orphan, b"truncated original progress stage").unwrap();
    std::fs::write(&retry_orphan, b"truncated previous recovery stage").unwrap();
    let published = publish_progress(&directory, &progress).unwrap();
    assert_eq!(
        std::fs::read(&published.path).unwrap(),
        json(&progress, MAX_PROGRESS_BYTES).unwrap()
    );
    assert_eq!(
        std::fs::read(&orphan).unwrap(),
        b"truncated original progress stage"
    );
    assert_eq!(
        std::fs::read(&retry_orphan).unwrap(),
        b"truncated previous recovery stage"
    );
    assert_eq!(publish_progress(&directory, &progress).unwrap(), published);
    std::fs::write(&published.path, b"corrupt final progress").unwrap();
    assert!(publish_progress(&directory, &progress).is_err());
    assert_eq!(
        std::fs::read(&published.path).unwrap(),
        b"corrupt final progress"
    );
    assert_eq!(
        std::fs::read(&orphan).unwrap(),
        b"truncated original progress stage"
    );
    assert_eq!(
        std::fs::read(&retry_orphan).unwrap(),
        b"truncated previous recovery stage"
    );
    for name in ["request.json", "source.json", "checkpoint.json"] {
        let stage = directory.join(format!(".{name}.stage"));
        std::fs::write(&stage, b"preserved non-progress partial").unwrap();
        assert!(publish(&directory, name, &vec![1u8, 2, 3], 1024).is_err());
        assert!(!directory.join(name).exists());
        assert_eq!(
            std::fs::read(stage).unwrap(),
            b"preserved non-progress partial"
        );
    }
}

/// Root supplies a fresh output directory and the exact real producer-backed
/// preparation pin. This test never creates a fake clean runtime certificate.
#[test]
#[ignore = "requires root-supplied pinned real BO3 update request via MTG_BO3_CONTINUATION_REQUEST"]
fn bo3_continuation_real_pinned_update_matches_oracle_and_checkpoint_only_recovery() {
    let path = PathBuf::from(
        std::env::var_os("MTG_BO3_CONTINUATION_REQUEST").expect("explicit root manifest"),
    );
    let request: Bo3GameplayUpdateRequestV1 = read(
        &existing_pin(&path, MAX_REQUEST_BYTES).unwrap(),
        MAX_REQUEST_BYTES,
    )
    .unwrap();
    assert!(
        !request.output_directory.exists(),
        "integration output must be fresh"
    );
    let parent = load_parent(&request.input).unwrap();
    let previous = prior(&request, &parent.origin, parent.completed).unwrap();
    let expected_updates = parent.completed.checked_add(1).unwrap();
    let expected_batches = previous
        .as_ref()
        .map_or(0, |p| p.attempt_progress.attempted_batches)
        .checked_add(1)
        .unwrap();
    let preparation_request = read_preparation(&request).unwrap();
    let expected_attempts = previous
        .as_ref()
        .map_or(0, |p| p.attempt_progress.ledger.len())
        .checked_add(preparation_request.attempts.len())
        .unwrap();
    assert_eq!(request.learning_rate_bits, parent.learning_rate_bits);
    assert_eq!(
        request.value_coefficient_bits,
        parent.value_coefficient_bits
    );
    let before = StateBits::capture(&parent.state).unwrap();
    let prepared = prepare_bo3_gameplay_batch_v1(preparation_request).unwrap();
    assert!(prepared.report_v1().eligible_matches > 0);
    assert!(
        prepared.groups.iter().any(|group| group.substeps.len() > 1),
        "real producer must witness multi-substep grouping"
    );
    let mut oracle = parent.state.clone();
    prepared
        .with_native_groups_v1(|groups, weights| {
            oracle.train_step_weighted_feature_transfer_v3(
                groups,
                weights,
                f32::from_bits(request.value_coefficient_bits),
                f32::from_bits(request.learning_rate_bits),
            )
        })
        .unwrap();
    drop(prepared);
    let expected = StateBits::capture(&oracle).unwrap();
    UPDATE_CALLS.with(|n| n.set(0));
    PREPARATION_CALLS.with(|n| n.set(0));
    STOP_BEFORE_PROGRESS.with(|flag| flag.set(true));
    assert!(
        update_bo3_gameplay_v1(request.clone())
            .unwrap_err()
            .contains("injected stop")
    );
    assert!(request.output_directory.join("checkpoint.json").is_file());
    assert!(!request.output_directory.join("progress.json").exists());
    assert_eq!(UPDATE_CALLS.with(|n| n.get()), 1);
    assert_eq!(PREPARATION_CALLS.with(|n| n.get()), 1);
    let checkpoint_before_recovery = existing_pin(
        &request.output_directory.join("checkpoint.json"),
        MAX_CHECKPOINT_BYTES,
    )
    .unwrap();
    let orphan = request.output_directory.join(".progress.json.stage");
    let retry_orphan = request
        .output_directory
        .join(".progress.json.stage-recovery-000000");
    std::fs::write(
        &orphan,
        b"injected truncated progress stage after committed checkpoint",
    )
    .unwrap();
    std::fs::write(&retry_orphan, b"injected truncated retry progress stage").unwrap();
    let recovered = update_bo3_gameplay_v1(request.clone()).unwrap();
    assert_eq!(
        UPDATE_CALLS.with(|n| n.get()),
        1,
        "checkpoint recovery must not call Adam"
    );
    assert_eq!(
        PREPARATION_CALLS.with(|n| n.get()),
        1,
        "checkpoint recovery must not replay preparation"
    );
    let (readback, checkpoint) = load_state(&recovered.learner.source).unwrap();
    assert_eq!(checkpoint.state, expected);
    assert_eq!(StateBits::capture(&readback.state).unwrap(), expected);
    assert_eq!(recovered.completed_bo3_updates, expected_updates);
    assert_eq!(recovered.attempted_batches, expected_batches);
    assert_eq!(recovered.attempted_matches, expected_attempts);
    assert_eq!(recovered.learner.identity.adam_step, before.adam_step + 1);
    assert_eq!(update_bo3_gameplay_v1(request.clone()).unwrap(), recovered);
    assert_eq!(UPDATE_CALLS.with(|n| n.get()), 1);
    assert_eq!(PREPARATION_CALLS.with(|n| n.get()), 1);
    assert_eq!(
        existing_pin(
            &request.output_directory.join("checkpoint.json"),
            MAX_CHECKPOINT_BYTES
        )
        .unwrap(),
        checkpoint_before_recovery
    );
    assert_eq!(
        std::fs::read(&orphan).unwrap(),
        b"injected truncated progress stage after committed checkpoint"
    );
    assert_eq!(
        std::fs::read(&retry_orphan).unwrap(),
        b"injected truncated retry progress stage"
    );
    assert_eq!(
        StateBits::capture(&load_parent(&request.input).unwrap().state).unwrap(),
        before,
        "immediate parent moments, step, gauge and parameters must remain frozen"
    );
    assert!(
        load_ordinary_bo3_parent_v1(&recovered.learner.source).is_err(),
        "ordinary reader must reject the new objective descriptor"
    );
    let mut changed = request;
    changed.learning_rate_bits ^= 1;
    assert!(update_bo3_gameplay_v1(changed).is_err());
}

#[test]
#[ignore = "requires a separate root-supplied all-incomplete ordinary or BO3 request via MTG_BO3_CONTINUATION_REQUEST"]
fn bo3_continuation_real_pinned_no_update_preserves_state_and_recovers_progress() {
    let path = PathBuf::from(
        std::env::var_os("MTG_BO3_CONTINUATION_REQUEST").expect("explicit root manifest"),
    );
    let request: Bo3GameplayUpdateRequestV1 = read(
        &existing_pin(&path, MAX_REQUEST_BYTES).unwrap(),
        MAX_REQUEST_BYTES,
    )
    .unwrap();
    assert!(
        !request.output_directory.exists(),
        "integration output must be fresh"
    );
    let parent = load_parent(&request.input).unwrap();
    let before = StateBits::capture(&parent.state).unwrap();
    let before_count = parent.completed;
    let previous_batches = request
        .previous_progress
        .as_ref()
        .map(|pin| {
            read::<Progress>(pin, MAX_PROGRESS_BYTES)
                .unwrap()
                .attempt_progress
                .attempted_batches
        })
        .unwrap_or(0);
    UPDATE_CALLS.with(|n| n.set(0));
    PREPARATION_CALLS.with(|n| n.set(0));
    let completed = update_bo3_gameplay_v1(request.clone()).unwrap();
    assert!(
        !completed.optimizer_updated,
        "this fixture requires real all-incomplete captured attempts"
    );
    assert_eq!(completed.learner, *request.input.learner_v1());
    assert_eq!(completed.completed_bo3_updates, before_count);
    assert_eq!(completed.attempted_batches, previous_batches + 1);
    assert!(!request.output_directory.join("checkpoint.json").exists());
    assert!(!request.output_directory.join("source.json").exists());
    assert_eq!(UPDATE_CALLS.with(|n| n.get()), 0);
    assert_eq!(PREPARATION_CALLS.with(|n| n.get()), 1);
    assert_eq!(update_bo3_gameplay_v1(request.clone()).unwrap(), completed);
    assert_eq!(UPDATE_CALLS.with(|n| n.get()), 0);
    assert_eq!(PREPARATION_CALLS.with(|n| n.get()), 1);
    assert_eq!(
        StateBits::capture(&load_parent(&request.input).unwrap().state).unwrap(),
        before
    );
}
