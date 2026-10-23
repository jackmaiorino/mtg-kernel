//! Training under the no-state-digest contract (lane
//! `no-digest-training-20261023`, DESIGN.md v2 section 5): the resolver, the
//! run-start conversion, continuation admission, the replay boundary, the
//! caches, and a converted run that collects, updates and resumes through
//! the driver with the per-forward check on. The train state's own checks
//! (compact equivalence, candidate refusal, worker MXCSR) are in
//! `native_policy_train_step_v1::no_state_digest_tests`.

use super::*;
use crate::expanded_deck_training_v1::loaded_source_cache_v1::{
    bypassed_for_test_v1, matches_for_test_v1,
};
use crate::expanded_deck_training_v1::{
    execute_v1, initialize_any_math_v1, load_expanded_inference_any_math_v1,
    load_expanded_update_inference_v1, loaded_source_cache_v1, ordered_update_preparation,
    ordinary_checkpoint_schema_v1, publish_json, trajectory_generation_v1, validate_trajectory,
    verify_collection_opponent_sources_v1, write_synthetic_fresh_source_with_parameters_v1,
    CollectionSamplerV1, ExpandedDeckListV1, ExpandedEpisodeV1, ExpandedLossSelectionV1,
    ExpandedTrainingCommandV1, ExpandedTrajectoryV1, ExpandedUpdateBackendV1, LineBUpdateOptionsV1,
    OptimizerMaskV1, UpdateBackwardExecutionV1, DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
    GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1,
};
use crate::native_expanded_training_run_v1::{
    run_native_expanded_training_v1, ExpandedOpponentAssignmentV1, ExpandedRunIterationV1,
    NativeExpandedTrainingRunV1, ScheduledExpandedEpisodeV1,
};
use crate::native_flat_tensorizer_v4::NativeFlatDecisionTensorV4;
use crate::sideboard_play_policy_v1::{
    FrozenPlayObservationTransferV3, FRESH_FEATURE_IDENTITY_V3, FRESH_FEATURE_IDENTITY_V4,
};
use std::path::PathBuf;

const DIGEST: std::ops::Range<usize> = STATE_DIGEST_COLUMNS_V1;

/// One invariant violation written into a snapshot.
type SnapshotEditV1 = fn(&mut NativePolicyValueTrainSnapshotV1);

/// One edit of the 1,403-wide conversion artifact's document.
type ArtifactEditV1 = fn(&mut Value);

/// Runs `body` on a 64 MiB thread. The nested converted and base loaders
/// keep several policies in each unoptimized debug frame, beyond the default
/// test thread's stack; release frames are far smaller.
fn on_large_stack(body: fn()) {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(body)
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "no-state-digest-training-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

/// A fresh-initialization source (V4) without a checkpoint.
fn fresh_source(root: &Path) -> ExpandedModelSourceV1 {
    let identity = FRESH_FEATURE_IDENTITY_V4;
    let descriptor =
        write_synthetic_fresh_source_with_parameters_v1(&root.join("source"), identity, |_| {});
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    let path = root.join("descriptor.json");
    fs::write(&path, &bytes).unwrap();
    ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path,
            sha256: sha(&bytes),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
    }
}

/// An ordinary GAE checkpoint of `policy` holding `snapshot`, with
/// `input_contract` and its digest when given.
fn checkpoint_of(
    policy: &FrozenPlayPolicyV1,
    snapshot: &NativePolicyValueTrainSnapshotV1,
    input_contract: Option<InputContractRecordV1>,
) -> ExpandedCheckpointV1 {
    let identity = policy.feature_identity_v1();
    let as_bits = |rows: &[crate::native_policy_value_net_v1::NativeNamedParameterV1]| {
        rows.iter().map(ParameterBitsV1::from_native).collect()
    };
    ExpandedCheckpointV1 {
        schema: ordinary_checkpoint_schema_v1(policy.identity_v1()).into(),
        feature_contract_digest: match &input_contract {
            Some(_) => converted_contract_digest_v1(identity.feature_contract_digest),
            None => identity.feature_contract_digest.into(),
        },
        feature_encoding_digest: identity.feature_encoding_digest.into(),
        card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
        source_import: policy.identity_v1().clone(),
        state_sha256: hex(&snapshot.state_sha256_v1().unwrap()),
        e_supported: None,
        soft_q: None,
        search_targets: None,
        adam_step: snapshot.adam_step,
        scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
        parameters: as_bits(&snapshot.parameters),
        first_moments: as_bits(&snapshot.first_moments),
        second_moments: as_bits(&snapshot.second_moments),
        trajectories: Vec::new(),
        loss_identity: GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into(),
        gamma_bits: Some(1.0_f32.to_bits()),
        gae_lambda_bits: Some(0.9_f32.to_bits()),
        entropy_coefficient_bits: Some(0.0_f32.to_bits()),
        exploration: None,
        learning_rate_bits: 0.0003_f32.to_bits(),
        value_coefficient_bits: 0.5_f32.to_bits(),
        registry_transfer: None,
        archive_distillation: None,
        training_math: TrainingMathV1::PlatformLibm,
        input_contract,
    }
}

fn tensor_mut<'a>(
    rows: &'a mut [crate::native_policy_value_net_v1::NativeNamedParameterV1],
    name: &str,
) -> &'a mut Vec<f32> {
    &mut rows
        .iter_mut()
        .find(|parameter| parameter.name == name)
        .unwrap()
        .values
}

/// `source` with a published checkpoint whose parameters, every moment
/// (the digest columns included) and Adam step have moved.
fn with_trained_checkpoint(root: &Path, source: &ExpandedModelSourceV1) -> ExpandedModelSourceV1 {
    let (policy, state) = initialize_any_math_v1(source).unwrap();
    let mut snapshot = state.snapshot_v1().unwrap();
    snapshot.adam_step = 5;
    for list in [&mut snapshot.first_moments, &mut snapshot.second_moments] {
        for (index, value) in tensor_mut(list, STATE_ENCODER_WEIGHT)
            .iter_mut()
            .enumerate()
        {
            *value = 1.0e-4 * (1 + index % 13) as f32;
        }
    }
    tensor_mut(&mut snapshot.parameters, STATE_ENCODER_WEIGHT)[5] += 0.03125;
    tensor_mut(&mut snapshot.first_moments, "value_head.0.bias")[2] = f32::from_bits(1);
    let saved = checkpoint_of(&policy, &snapshot, None);
    let directory = root.join("parent");
    fs::create_dir_all(&directory).unwrap();
    ExpandedModelSourceV1 {
        checkpoint: Some(publish_json(&directory, "checkpoint.json", &saved).unwrap()),
        ..source.clone()
    }
}

/// Converts `source` into `out_dir` and returns the outputs and the
/// converted source.
fn convert(
    root: &Path,
    source: &ExpandedModelSourceV1,
    out_dir: &str,
) -> (NoStateDigestOutputsV1, ExpandedModelSourceV1) {
    let path = root.join(format!("{out_dir}-source.json"));
    fs::write(&path, serde_json::to_vec(source).unwrap()).unwrap();
    let outputs = convert_no_state_digest_v1(&path, &root.join(out_dir)).unwrap();
    let converted: ExpandedModelSourceV1 = read_pinned(&outputs.source).unwrap();
    (outputs, converted)
}

/// The loader's refusal of `source`.
fn refusal(source: &ExpandedModelSourceV1) -> String {
    match initialize_any_math_v1(source) {
        Ok(_) => panic!("the loader admitted the source"),
        Err(error) => error,
    }
}

fn columns(values: &[f32]) -> Vec<u32> {
    values
        .chunks_exact(STATE_ENCODER_INPUT_V1)
        .flat_map(|row| row[DIGEST].iter().map(|v| v.to_bits()))
        .collect()
}

fn kept(values: &[f32]) -> Vec<u32> {
    values
        .chunks_exact(STATE_ENCODER_INPUT_V1)
        .flat_map(|row| {
            row[..DIGEST.start]
                .iter()
                .chain(&row[DIGEST.end..])
                .map(|v| v.to_bits())
        })
        .collect()
}

#[test]
fn the_resolver_admits_exactly_the_whole_pairs_and_their_own_conversions() {
    let cards = format!("{KERNEL_CARDDB_HASH:016x}");
    let pin = PinnedFileV1 {
        path: "conversion.json".into(),
        sha256: "c".repeat(64),
    };
    let record = InputContractRecordV1::of_conversion_v1(pin.clone());
    for identity in [FRESH_FEATURE_IDENTITY_V3, FRESH_FEATURE_IDENTITY_V4] {
        let base = identity.feature_contract_digest;
        let encoding = identity.feature_encoding_digest;
        let plain = resolve_contract_v1(base, encoding, &cards, None).unwrap();
        assert_eq!(plain.generation, identity.generation);
        assert_eq!(plain.effective, EffectiveContractV1::None);
        let converted = converted_contract_digest_v1(base);
        let resolved = resolve_contract_v1(&converted, encoding, &cards, Some(&record)).unwrap();
        // A converted V4 identity dispatches as V4.
        assert_eq!(resolved.generation, identity.generation);
        assert_eq!(resolved.base_contract_digest, base);
        assert_eq!(resolved.base_encoding_digest, encoding);
        assert_eq!(
            resolved.input_contract_digest_v1(),
            Some(converted.as_str())
        );
        // The marker and the digest only together.
        assert!(resolve_contract_v1(&converted, encoding, &cards, None).is_err());
        assert!(resolve_contract_v1(base, encoding, &cards, Some(&record)).is_err());
        assert!(resolve_contract_v1(&converted, encoding, "0", Some(&record)).is_err());
        let mut other = record.clone();
        other.state_digest_columns = [123, 218];
        assert!(resolve_contract_v1(&converted, encoding, &cards, Some(&other)).is_err());
        other = record.clone();
        other.contract = "no-state-digest-v2".into();
        assert!(resolve_contract_v1(&converted, encoding, &cards, Some(&other)).is_err());
    }
    // Never across generations.
    let v3 = converted_contract_digest_v1(FRESH_FEATURE_IDENTITY_V3.feature_contract_digest);
    assert!(resolve_contract_v1(
        &v3,
        FRESH_FEATURE_IDENTITY_V4.feature_encoding_digest,
        &cards,
        Some(&record)
    )
    .is_err());
}

/// The run-start conversion keeps the Adam step, gauge anchor, training math
/// and every kept bit of the parameters and both moments, zeroes exactly the
/// digest columns, agrees with the 1,403-wide artifact, and its receipt
/// describes the converted optimizer state. A tampered artifact is refused,
/// and the loaded-source cache never holds the converted source.
#[test]
fn conversion_keeps_the_optimizer_and_every_kept_bit() {
    on_large_stack(conversion_keeps_the_optimizer_and_every_kept_bit_v1);
}

fn conversion_keeps_the_optimizer_and_every_kept_bit_v1() {
    let root = temp_root("conversion");
    let parent = with_trained_checkpoint(&root, &fresh_source(&root));
    let (outputs, converted) = convert(&root, &parent, "conversion");
    let (base_policy, base_state) = initialize_any_math_v1(&parent).unwrap();
    let (policy, state) = initialize_any_math_v1(&converted).unwrap();
    let base = base_state.snapshot_v1().unwrap();
    let after = state.snapshot_v1().unwrap();
    assert_eq!(after.adam_step, 5);
    assert_eq!(after.adam_step, base.adam_step);
    assert_eq!(after.scorer_bias_anchor_bits, base.scorer_bias_anchor_bits);
    assert_eq!(state.training_math_v1(), base_state.training_math_v1());
    assert_eq!(state.no_state_digest_columns_v1(), Some(DIGEST));
    for (a, b) in [
        (&after.parameters, &base.parameters),
        (&after.first_moments, &base.first_moments),
        (&after.second_moments, &base.second_moments),
    ] {
        for (x, y) in a.iter().zip(b) {
            if x.name == STATE_ENCODER_WEIGHT {
                assert_eq!(kept(&x.values), kept(&y.values));
                assert!(columns(&x.values).iter().all(|bits| *bits == 0));
                assert!(columns(&y.values).iter().any(|bits| *bits != 0));
            } else {
                assert_eq!(x, y);
            }
        }
    }
    assert_eq!(
        policy.input_contract_v1(),
        Some(outputs.record.converted.feature_contract_digest.as_str())
    );
    assert_eq!(
        policy.input_contract_conversion_v1(),
        Some(&outputs.conversion)
    );
    assert_eq!(
        policy.actual_model_identity_v1().model_parameter_sha256,
        outputs.record.converted.model_parameter_sha256
    );
    assert_eq!(
        base_policy
            .actual_model_identity_v1()
            .model_parameter_sha256,
        outputs.record.base.model_parameter_sha256
    );
    // The receipt describes the converted state, not the base one.
    let (_, identity) = super::super::load_expanded_inference_v1(&converted).unwrap();
    assert_eq!(
        identity.state_sha256,
        hex(&after.state_sha256_v1().unwrap())
    );
    assert_ne!(identity.state_sha256, hex(&base.state_sha256_v1().unwrap()));
    // Cached under the load's own per-forward check setting.
    let bytes = read_pinned_bytes(&converted.play_import).unwrap();
    with_check_for_test_v1(false, || {
        initialize_any_math_v1(&converted).unwrap();
        assert!(loaded_source_cache_v1::lookup_v1(&converted, &bytes, None)
            .unwrap()
            .is_some());
    });
    // The narrow artifact, edited and republished with its enclosing pins:
    // a kept bit, any tensor's shape (the 1,403-wide state encoder included)
    // and its identity members are all checked.
    let tampered = |name: &str, edit: fn(&mut Value)| {
        let mut document: Value =
            read_pinned(outputs.converted_checkpoint.as_ref().unwrap()).unwrap();
        edit(&mut document);
        let directory = root.join("tampered").join(name);
        fs::create_dir_all(&directory).unwrap();
        let mut record = outputs.record.clone();
        record.converted_checkpoint =
            Some(publish_json(&directory, "checkpoint.json", &document).unwrap());
        let import = NoStateDigestImportV1 {
            schema: IMPORT_SCHEMA_V1.into(),
            base_play_import: parent.play_import.clone(),
            conversion: publish_json(&directory, "conversion.json", &record).unwrap(),
        };
        ExpandedModelSourceV1 {
            play_import: publish_json(&directory, "import.json", &import).unwrap(),
            ..converted.clone()
        }
    };
    fn tensor<'a>(document: &'a mut Value, list: &str, name: &str) -> &'a mut Value {
        document[list]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["name"] == name)
            .unwrap()
    }
    // An unchanged republication is admitted.
    initialize_any_math_v1(&tampered("unchanged", |_| {})).unwrap();
    let cases: [(&str, ArtifactEditV1, &str); 9] = [
        (
            "kept-bit",
            |d| {
                tensor(d, "first_moments", STATE_ENCODER_WEIGHT)["values"][3] =
                    Value::from(1.0e-3_f32.to_bits())
            },
            "converted checkpoint differs from the converted state",
        ),
        (
            "weight-flat",
            |d| tensor(d, "parameters", STATE_ENCODER_WEIGHT)["shape"] = serde_json::json!([1]),
            "converted checkpoint tensor shape differs",
        ),
        (
            "weight-padded",
            |d| {
                tensor(d, "parameters", STATE_ENCODER_WEIGHT)["shape"][1] =
                    Value::from(STATE_ENCODER_INPUT_V1)
            },
            "converted checkpoint tensor shape differs",
        ),
        (
            "moment-shape",
            |d| {
                tensor(d, "second_moments", STATE_ENCODER_WEIGHT)["shape"] =
                    serde_json::json!([1_403, 64])
            },
            "converted checkpoint tensor shape differs",
        ),
        (
            "other-shape",
            |d| tensor(d, "parameters", "value_head.2.bias")["shape"] = serde_json::json!([1, 1]),
            "converted checkpoint tensor shape differs",
        ),
        (
            "schema",
            |d| d["schema"] = Value::from("mtg-kernel-expanded-deck-checkpoint/v1"),
            "the converted checkpoint's schema, encoding or card database differs",
        ),
        (
            "encoding",
            |d| {
                d["feature_encoding_digest"] =
                    Value::from(FRESH_FEATURE_IDENTITY_V3.feature_encoding_digest)
            },
            "the converted checkpoint's schema, encoding or card database differs",
        ),
        (
            "cards",
            |d| d["card_db_hash"] = Value::from("0000000000000000"),
            "the converted checkpoint's schema, encoding or card database differs",
        ),
        (
            "contract",
            |d| d["input_contract"]["state_digest_columns"] = serde_json::json!([123, 218]),
            "the converted checkpoint names another base state or optimizer",
        ),
    ];
    for (name, edit, expected) in cases {
        assert_eq!(refusal(&tampered(name, edit)), expected, "{name}");
    }
    // A missing identity member is refused by the artifact's reader.
    for (name, member) in [
        ("no-schema", "schema"),
        ("no-encoding", "feature_encoding_digest"),
        ("no-cards", "card_db_hash"),
    ] {
        let source = tampered(name, |_| {});
        let conversion: NoStateDigestConversionV1 = read_pinned(
            &read_pinned::<NoStateDigestImportV1>(&source.play_import)
                .unwrap()
                .conversion,
        )
        .unwrap();
        let pin = conversion.converted_checkpoint.clone().unwrap();
        let mut document: Value = read_pinned(&pin).unwrap();
        document.as_object_mut().unwrap().remove(member);
        // Republish the edited artifact through the record and the import.
        let directory = pin.path.parent().unwrap();
        let mut record = conversion.clone();
        record.converted_checkpoint =
            Some(publish_json(directory, "checkpoint-edited.json", &document).unwrap());
        let import = NoStateDigestImportV1 {
            schema: IMPORT_SCHEMA_V1.into(),
            base_play_import: parent.play_import.clone(),
            conversion: publish_json(directory, "conversion-repinned.json", &record).unwrap(),
        };
        let source = ExpandedModelSourceV1 {
            play_import: publish_json(directory, "import-repinned.json", &import).unwrap(),
            ..converted.clone()
        };
        assert!(
            refusal(&source).contains(member),
            "{name}: {}",
            refusal(&source)
        );
    }
    fs::remove_dir_all(root).unwrap();
}

/// A continuation admits only a checkpoint of this exact conversion with +0.0
/// W/m/v digest columns and no -0.0 bias, and an ordinary source refuses a
/// converted checkpoint.
#[test]
fn continuation_admits_only_this_conversion_with_clean_columns() {
    on_large_stack(continuation_admits_only_this_conversion_with_clean_columns_v1);
}

fn continuation_admits_only_this_conversion_with_clean_columns_v1() {
    let root = temp_root("continuation");
    let parent = with_trained_checkpoint(&root, &fresh_source(&root));
    let (outputs, converted) = convert(&root, &parent, "conversion-a");
    let (_, other) = convert(&root, &parent, "conversion-b");
    assert_ne!(converted.play_import, other.play_import);
    let (policy, state) = initialize_any_math_v1(&converted).unwrap();
    let record = policy_contract_record_v1(&policy).unwrap().unwrap();
    assert_eq!(record.conversion, outputs.conversion);
    // A trained continuation: moved kept parameters, moments and step.
    let mut trained = state.snapshot_v1().unwrap();
    trained.adam_step += 1;
    tensor_mut(&mut trained.parameters, STATE_ENCODER_WEIGHT)[7] -= 0.0625;
    tensor_mut(&mut trained.second_moments, STATE_ENCODER_WEIGHT)[300] = 2.0e-5;
    let directory = root.join("trained");
    fs::create_dir_all(&directory).unwrap();
    let publish = |name: &str, saved: &ExpandedCheckpointV1| ExpandedModelSourceV1 {
        checkpoint: Some(publish_json(&directory, name, saved).unwrap()),
        ..converted.clone()
    };
    let saved = checkpoint_of(&policy, &trained, Some(record.clone()));
    let continuation = publish("checkpoint.json", &saved);
    let (resumed_policy, resumed) = initialize_any_math_v1(&continuation).unwrap();
    assert_eq!(resumed.snapshot_v1().unwrap(), trained);
    assert_eq!(resumed.no_state_digest_columns_v1(), Some(DIGEST));
    assert_eq!(
        policy_contract_record_v1(&resumed_policy).unwrap(),
        Some(record.clone())
    );
    // Through the update's loader with its input binding.
    let identity =
        super::super::load_expanded_update_inference_v1(&continuation, &[], 0.0003, 0.5).unwrap();
    assert_eq!(identity.state_sha256, saved.state_sha256);
    assert!(
        super::super::load_expanded_update_inference_v1(&continuation, &[], 0.0001, 0.5).is_err()
    );

    // The same checkpoint through the ordinary import, and through another
    // conversion of the same import.
    let ordinary = ExpandedModelSourceV1 {
        play_import: parent.play_import.clone(),
        ..continuation.clone()
    };
    assert_eq!(
        refusal(&ordinary),
        "record and model no-state-digest contracts differ"
    );
    let crossed = ExpandedModelSourceV1 {
        play_import: other.play_import.clone(),
        ..continuation.clone()
    };
    assert_eq!(
        refusal(&crossed),
        "the checkpoint belongs to another no-state-digest conversion"
    );
    // An unconverted checkpoint through the converted import.
    let (base_policy, base_state) = initialize_any_math_v1(&parent).unwrap();
    let mut unconverted = base_state.snapshot_v1().unwrap();
    unconverted.adam_step += 1;
    let plain = publish(
        "unconverted.json",
        &checkpoint_of(&base_policy, &unconverted, None),
    );
    assert_eq!(
        refusal(&plain),
        "an unconverted checkpoint does not continue a no-state-digest source"
    );
    // An explicit null member is not absence: refused, as the strict reader
    // refused the then unknown member.
    let mut document =
        serde_json::to_value(checkpoint_of(&base_policy, &unconverted, None)).unwrap();
    assert!(document.get("input_contract").is_none());
    assert!(serde_json::from_value::<ExpandedCheckpointV1>(document.clone()).is_ok());
    document["input_contract"] = Value::Null;
    assert!(serde_json::from_value::<ExpandedCheckpointV1>(document.clone()).is_err());
    let null_member = ExpandedModelSourceV1 {
        checkpoint: Some(publish_json(&directory, "null-member.json", &document).unwrap()),
        ..parent.clone()
    };
    assert!(
        refusal(&null_member).contains("null"),
        "{}",
        refusal(&null_member)
    );
    // The marker with the base digest.
    let mut relabelled = saved.clone();
    relabelled.feature_contract_digest = base_policy
        .feature_identity_v1()
        .feature_contract_digest
        .into();
    assert_eq!(
        refusal(&publish("relabelled.json", &relabelled)),
        "successor feature identity differs from its no-state-digest contract"
    );
    // Dirty columns and a -0.0 bias, never repaired.
    let cases: [(&str, SnapshotEditV1, &str); 5] = [
        (
            "w.json",
            |s| tensor_mut(&mut s.parameters, STATE_ENCODER_WEIGHT)[130] = 0.25,
            "no-state-digest contract: a stored state-digest parameter column is not +0.0",
        ),
        (
            "w-negative-zero.json",
            |s| tensor_mut(&mut s.parameters, STATE_ENCODER_WEIGHT)[1_499 + 200] = -0.0,
            "no-state-digest contract: a stored state-digest parameter column is not +0.0",
        ),
        (
            "m.json",
            |s| tensor_mut(&mut s.first_moments, STATE_ENCODER_WEIGHT)[123] = -0.0,
            "no-state-digest contract: a stored state-digest first moment column is not +0.0",
        ),
        (
            "v.json",
            |s| tensor_mut(&mut s.second_moments, STATE_ENCODER_WEIGHT)[218] = 1.0e-9,
            "no-state-digest contract: a stored state-digest second moment column is not +0.0",
        ),
        (
            "bias.json",
            |s| tensor_mut(&mut s.parameters, STATE_ENCODER_BIAS)[4] = -0.0,
            "a state encoder bias of -0.0 has no exact no-state-digest conversion",
        ),
    ];
    for (name, mutate, expected) in cases {
        let mut dirty = trained.clone();
        mutate(&mut dirty);
        let source = publish(name, &checkpoint_of(&policy, &dirty, Some(record.clone())));
        assert_eq!(refusal(&source), expected, "{name}");
    }
    fs::remove_dir_all(root).unwrap();
}

/// A base that cannot train (here a fresh registry-transfer source) keeps
/// the inference-only install that predates training: it converts and loads
/// for inference with the contract, and the training loader refuses it
/// before loading anything else.
#[test]
fn inference_only_bases_keep_the_converted_inference_install() {
    on_large_stack(inference_only_bases_keep_the_converted_inference_install_v1);
}

fn inference_only_bases_keep_the_converted_inference_install_v1() {
    let root = temp_root("inference-only");
    let fixture =
        crate::expanded_deck_training_v1::fresh_registry_transfer_source::tests::fixture();
    let (outputs, converted) = convert(&root, &fixture.source, "conversion");
    assert!(outputs.converted_checkpoint.is_none());
    let (policy, identity) = super::super::load_expanded_inference_v1(&converted).unwrap();
    assert_eq!(
        policy.input_contract_v1(),
        Some(outputs.record.converted.feature_contract_digest.as_str())
    );
    assert_eq!(
        identity.model.model_parameter_sha256,
        outputs.record.converted.model_parameter_sha256
    );
    // No training records: the contract names no conversion.
    assert_eq!(policy.input_contract_conversion_v1(), None);
    assert_eq!(
        refusal(&converted),
        "this no-state-digest source's base is inference-only and does not train"
    );
    drop(fixture);
    fs::remove_dir_all(root).unwrap();
}

/// A published continuation of `converted`: moved kept parameters, moments
/// and step, under its contract member.
fn with_continuation(root: &Path, converted: &ExpandedModelSourceV1) -> ExpandedModelSourceV1 {
    let (policy, state) = initialize_any_math_v1(converted).unwrap();
    let mut trained = state.snapshot_v1().unwrap();
    trained.adam_step += 1;
    tensor_mut(&mut trained.parameters, STATE_ENCODER_WEIGHT)[9] += 0.125;
    tensor_mut(&mut trained.first_moments, STATE_ENCODER_WEIGHT)[400] = -3.0e-4;
    let record = policy_contract_record_v1(&policy).unwrap();
    let directory = root.join("continuation");
    fs::create_dir_all(&directory).unwrap();
    ExpandedModelSourceV1 {
        checkpoint: Some(
            publish_json(
                &directory,
                "checkpoint.json",
                &checkpoint_of(&policy, &trained, record),
            )
            .unwrap(),
        ),
        ..converted.clone()
    }
}

/// Two loads agree exactly: state, model, contract, conversion, check model
/// and receipt.
fn assert_same_converted_load(
    actual: &(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1),
    expected: &(FrozenPlayPolicyV1, NativePolicyValueTrainStateV1),
    source: &ExpandedModelSourceV1,
) {
    let ((policy, state), (expected_policy, expected_state)) = (actual, expected);
    assert_eq!(
        state.snapshot_v1().unwrap(),
        expected_state.snapshot_v1().unwrap()
    );
    assert_eq!(state.training_math_v1(), expected_state.training_math_v1());
    assert_eq!(
        state.no_state_digest_columns_v1(),
        expected_state.no_state_digest_columns_v1()
    );
    assert_eq!(
        policy.actual_model_identity_v1(),
        expected_policy.actual_model_identity_v1()
    );
    assert_eq!(policy.identity_v1(), expected_policy.identity_v1());
    assert_eq!(
        policy.input_contract_v1(),
        expected_policy.input_contract_v1()
    );
    assert_eq!(
        policy.input_contract_conversion_v1(),
        expected_policy.input_contract_conversion_v1()
    );
    assert_eq!(
        policy.no_state_digest_check_v1(),
        expected_policy.no_state_digest_check_v1()
    );
    assert_eq!(
        super::super::inference_identity_v1(source, policy, state).unwrap(),
        super::super::inference_identity_v1(source, expected_policy, expected_state).unwrap()
    );
    policy.duplicate_loaded_v1().unwrap();
}

/// The loaded-source cache serves converted run starts and continuations:
/// a hit equals a full load, binds the update's inputs, matches only the
/// per-forward check setting it loaded under, and a changed or missing
/// input fails a hit with the full load's error.
#[test]
fn converted_loads_are_served_exactly_by_the_loaded_source_cache() {
    on_large_stack(converted_loads_are_served_exactly_by_the_loaded_source_cache_v1);
}

fn converted_loads_are_served_exactly_by_the_loaded_source_cache_v1() {
    let root = temp_root("loaded-cache");
    let parent = with_trained_checkpoint(&root, &fresh_source(&root));
    let (outputs, converted) = convert(&root, &parent, "conversion");
    let continuation = with_continuation(&root, &converted);
    let base_descriptor: super::super::ExpandedFreshInitializationSourceV1 =
        read_pinned(&parent.play_import).unwrap();
    for check in [false, true] {
        with_check_for_test_v1(check, || {
            for source in [&converted, &continuation] {
                let full = bypassed_for_test_v1(|| initialize_any_math_v1(source)).unwrap();
                assert_eq!(full.0.no_state_digest_check_v1(), check);
                let first = initialize_any_math_v1(source).unwrap();
                let before = matches_for_test_v1();
                let second = initialize_any_math_v1(source).unwrap();
                assert_eq!(matches_for_test_v1(), before + 1, "a converted hit");
                assert_same_converted_load(&first, &full, source);
                assert_same_converted_load(&second, &full, source);
            }
        });
    }
    with_check_for_test_v1(false, || {
        // An entry kept under the check is not served without it.
        let before = matches_for_test_v1();
        let (policy, _) = initialize_any_math_v1(&continuation).unwrap();
        assert!(!policy.no_state_digest_check_v1());
        assert!(
            matches_for_test_v1() <= before + 1,
            "at most the base's own hit"
        );
        // The update's input binding, on a hit as on a full load.
        let load = |learning_rate: f32| {
            load_expanded_update_inference_v1(&continuation, &[], learning_rate, 0.5)
        };
        let full = bypassed_for_test_v1(|| load(0.0003)).unwrap();
        let before = matches_for_test_v1();
        assert_eq!(load(0.0003).unwrap(), full);
        assert_eq!(matches_for_test_v1(), before + 1);
        let full = bypassed_for_test_v1(|| load(0.0001)).unwrap_err();
        let before = matches_for_test_v1();
        assert_eq!(load(0.0001).unwrap_err(), full);
        assert_eq!(matches_for_test_v1(), before + 1);
        assert_eq!(full, "checkpoint inputs or optimizer settings differ");
        // A run start with update inputs loads in full, with its result.
        let load = || load_expanded_update_inference_v1(&converted, &[], 0.0003, 0.5);
        assert_eq!(load(), bypassed_for_test_v1(load));

        // Every input of a converted load: a changed or missing file fails a
        // hit with exactly the full load's error.
        let files = [
            (
                &converted,
                outputs.converted_checkpoint.clone().unwrap().path,
            ),
            (&converted, outputs.conversion.path.clone()),
            (&continuation, outputs.conversion.path.clone()),
            (&continuation, parent.play_import.path.clone()),
            (&continuation, base_descriptor.parameters.path.clone()),
            (&continuation, base_descriptor.initialization.path.clone()),
            (&continuation, parent.checkpoint.clone().unwrap().path),
            (&continuation, continuation.checkpoint.clone().unwrap().path),
        ];
        for (source, path) in files {
            let reference = initialize_any_math_v1(source).unwrap();
            let original = fs::read(&path).unwrap();
            let mut changed = original.clone();
            changed[original.len() / 2] ^= 1;
            for (label, bytes) in [("changed", Some(changed)), ("missing", None)] {
                match bytes {
                    Some(bytes) => fs::write(&path, bytes).unwrap(),
                    None => fs::remove_file(&path).unwrap(),
                }
                let full = bypassed_for_test_v1(|| refusal(source));
                let before = matches_for_test_v1();
                let cached = refusal(source);
                assert_eq!(
                    matches_for_test_v1(),
                    before + 1,
                    "{label} {}",
                    path.display()
                );
                assert_eq!(cached, full, "{label} {}", path.display());
                fs::write(&path, &original).unwrap();
            }
            let before = matches_for_test_v1();
            let restored = initialize_any_math_v1(source).unwrap();
            assert_eq!(matches_for_test_v1(), before + 1);
            assert_same_converted_load(&restored, &reference, source);
        }
    });
    fs::remove_dir_all(root).unwrap();
}

/// A converted opponent whose base trains is verified at the collection
/// boundary and then served by the process opponent cache, like an ordinary
/// one: a hit equals a full load, matches only its per-forward check
/// setting, and a changed input fails the boundary check.
#[test]
fn converted_opponents_are_verified_and_served_exactly_by_the_opponent_cache() {
    on_large_stack(converted_opponents_are_verified_and_served_exactly_by_the_opponent_cache_v1);
}

fn converted_opponents_are_verified_and_served_exactly_by_the_opponent_cache_v1() {
    let root = temp_root("opponent-cache");
    let parent = with_trained_checkpoint(&root, &fresh_source(&root));
    let (outputs, converted) = convert(&root, &parent, "conversion");
    let continuation = with_continuation(&root, &converted);
    for source in [&converted, &continuation] {
        let episodes = vec![episode("opponent", 2_026_102_401, Some(source.clone()))];
        assert_eq!(
            verify_collection_opponent_sources_v1(&episodes).unwrap(),
            vec![source.clone()]
        );
        with_check_for_test_v1(false, || {
            let (first, _) = ordered_update_preparation::load_opponent_cached_v1(source).unwrap();
            let (second, hit) =
                ordered_update_preparation::load_opponent_cached_v1(source).unwrap();
            assert!(hit);
            let full =
                bypassed_for_test_v1(|| load_expanded_inference_any_math_v1(source)).unwrap();
            for loaded in [&first, &second] {
                assert_eq!(loaded.behavior.identity, full.1);
                assert_eq!(
                    loaded.policy.actual_model_identity_v1(),
                    full.0.actual_model_identity_v1()
                );
                assert_eq!(
                    loaded.policy.input_contract_v1(),
                    full.0.input_contract_v1()
                );
                assert_eq!(
                    loaded.policy.input_contract_conversion_v1(),
                    full.0.input_contract_conversion_v1()
                );
                assert!(!loaded.policy.no_state_digest_check_v1());
            }
        });
        with_check_for_test_v1(true, || {
            let (loaded, hit) =
                ordered_update_preparation::load_opponent_cached_v1(source).unwrap();
            assert!(
                !hit,
                "an entry kept without the check is not served with it"
            );
            assert!(loaded.policy.no_state_digest_check_v1());
        });
    }
    // A changed input fails the boundary check, as it fails a full load.
    let episodes = vec![episode("opponent", 2_026_102_401, Some(converted.clone()))];
    for path in [
        outputs.converted_checkpoint.clone().unwrap().path,
        outputs.conversion.path.clone(),
        parent.checkpoint.clone().unwrap().path,
    ] {
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed[original.len() / 2] ^= 1;
        fs::write(&path, changed).unwrap();
        assert!(verify_collection_opponent_sources_v1(&episodes).is_err());
        assert!(bypassed_for_test_v1(|| load_expanded_inference_any_math_v1(&converted)).is_err());
        fs::write(&path, &original).unwrap();
    }
    assert_eq!(
        verify_collection_opponent_sources_v1(&episodes).unwrap(),
        vec![converted.clone()]
    );
    fs::remove_dir_all(root).unwrap();
}

/// A converted source over a phase-1 archive-distillation base checkpoint
/// (whose full load reads its named parent) is never served by either
/// cache: its run start and its continuation load in full every time, the
/// collection boundary leaves it to the full loader, and a corrupted or
/// deleted parent is refused exactly as a full load refuses it.
#[test]
fn archive_distillation_bases_always_load_in_full() {
    on_large_stack(archive_distillation_bases_always_load_in_full_v1);
}

fn archive_distillation_bases_always_load_in_full_v1() {
    use crate::expanded_deck_training_v1::archive_distillation_v1::{
        ArchiveDistillationArmV1, ArchiveDistillationProvenanceV1, PHASE1_LOSS_IDENTITY_V1,
    };
    let root = temp_root("archive-base");
    let parent = with_trained_checkpoint(&root, &fresh_source(&root));
    let parent_pin = parent.checkpoint.clone().unwrap();
    let parent_saved: ExpandedCheckpointV1 = read_pinned(&parent_pin).unwrap();
    // A phase-1 checkpoint: the parent's optimizer state, moved parameters.
    let (policy, state) = initialize_any_math_v1(&parent).unwrap();
    let mut snapshot = state.snapshot_v1().unwrap();
    tensor_mut(&mut snapshot.parameters, "value_head.2.bias")[0] += 0.0625;
    let mut phase1 = checkpoint_of(&policy, &snapshot, None);
    phase1.loss_identity = PHASE1_LOSS_IDENTITY_V1.into();
    phase1.gamma_bits = None;
    phase1.gae_lambda_bits = None;
    phase1.entropy_coefficient_bits = None;
    phase1.learning_rate_bits = 1e-4_f32.to_bits();
    phase1.value_coefficient_bits = 0.0_f32.to_bits();
    let pin = |name: &str, fill: char| PinnedFileV1 {
        path: root.join(name),
        sha256: fill.to_string().repeat(64),
    };
    phase1.archive_distillation = Some(ArchiveDistillationProvenanceV1 {
        method: "spy-archive-distillation-phase1-v1".into(),
        lineage: "r1".into(),
        arm: ArchiveDistillationArmV1::Uniform,
        parent: parent_pin.clone(),
        parent_state_sha256: parent_saved.state_sha256.clone(),
        games: pin("games.jsonl", 'd'),
        archive_sources: vec![pin("ARCHIVE.jsonl", '5')],
        bank: pin("bank.json", 'e'),
        bank_rows_sha256: "f".repeat(64),
        anchor_sha256: "1".repeat(64),
        steps: 1,
        learning_rate_bits: 1e-4_f32.to_bits(),
        phase1_state_sha256: "2".repeat(64),
        runtime_commit: "test".into(),
        cell: None,
        update_scope: Default::default(),
        rules_pathway: None,
    });
    let directory = root.join("phase1");
    fs::create_dir_all(&directory).unwrap();
    let base = ExpandedModelSourceV1 {
        checkpoint: Some(publish_json(&directory, "checkpoint.json", &phase1).unwrap()),
        ..parent.clone()
    };
    initialize_any_math_v1(&base).unwrap();
    let (_, converted) = convert(&root, &base, "conversion");
    let continuation = with_continuation(&root, &converted);
    let corrupt = |action: &str| {
        let original = fs::read(&parent_pin.path).unwrap();
        match action {
            "changed" => {
                let mut changed = original.clone();
                changed[original.len() / 2] ^= 1;
                fs::write(&parent_pin.path, changed).unwrap();
            }
            _ => fs::remove_file(&parent_pin.path).unwrap(),
        }
        original
    };
    with_check_for_test_v1(false, || {
        for source in [&converted, &continuation] {
            let bytes = read_pinned_bytes(&source.play_import).unwrap();
            let reference = initialize_any_math_v1(source).unwrap();
            let before = matches_for_test_v1();
            let again = initialize_any_math_v1(source).unwrap();
            assert_eq!(matches_for_test_v1(), before, "no cache served it");
            assert_same_converted_load(&again, &reference, source);
            assert!(loaded_source_cache_v1::lookup_v1(source, &bytes, None)
                .unwrap()
                .is_none());
            // The collection boundary leaves it to the full loader.
            let episodes = vec![episode("archive", 2_026_102_501, Some(source.clone()))];
            let verified = verify_collection_opponent_sources_v1(&episodes).unwrap();
            assert!(verified.is_empty());
            let opponents = || super::super::OpponentCacheV1::for_collection(verified.clone());
            opponents().load(source).unwrap();
            for action in ["changed", "missing"] {
                let original = corrupt(action);
                let full = bypassed_for_test_v1(|| refusal(source));
                assert_eq!(refusal(source), full, "{action}");
                let opponent = match opponents().load(source) {
                    Ok(_) => panic!("{action}: the opponent loaded"),
                    Err(error) => error,
                };
                let full_opponent =
                    match bypassed_for_test_v1(|| load_expanded_inference_any_math_v1(source)) {
                        Ok(_) => panic!("{action}: the full opponent load succeeded"),
                        Err(error) => error,
                    };
                assert_eq!(opponent, full_opponent, "{action}");
                fs::write(&parent_pin.path, &original).unwrap();
            }
            assert_same_converted_load(
                &initialize_any_math_v1(source).unwrap(),
                &reference,
                source,
            );
        }
    });
    fs::remove_dir_all(root).unwrap();
}

fn list(id: &str) -> ExpandedDeckListV1 {
    let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let cards = registration.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    }
}

fn episode(id: &str, seed: u64, opponent: Option<ExpandedModelSourceV1>) -> ExpandedEpisodeV1 {
    let decks = [list("Affinity"), list("Terror")];
    ExpandedEpisodeV1 {
        id: id.into(),
        seed,
        starting_player: 0,
        learner_seat: 0,
        opponent,
        opponent_kind: None,
        opponent_search: None,
        registered: decks.clone(),
        selected: decks,
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
    }
}

fn gae() -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
    }
}

fn run_config(
    initial_source: &ExpandedModelSourceV1,
    workers: usize,
    output_directory: PathBuf,
) -> NativeExpandedTrainingRunV1 {
    let scheduled = |iteration: u64, opponent| ScheduledExpandedEpisodeV1 {
        episode: episode(
            &format!("converted-iter-{iteration}"),
            2_026_102_311 + iteration,
            None,
        ),
        opponent,
    };
    NativeExpandedTrainingRunV1 {
        schema: "mtg-kernel-native-expanded-training-run/v1".into(),
        initial_source: initial_source.clone(),
        opponents: vec![],
        iterations: vec![
            ExpandedRunIterationV1 {
                episodes: vec![scheduled(0, ExpandedOpponentAssignmentV1::Current)],
            },
            ExpandedRunIterationV1 {
                episodes: vec![scheduled(1, ExpandedOpponentAssignmentV1::Initial)],
            },
        ],
        learning_rate: 0.0003,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        loss_selection: gae(),
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        collection_workers: workers,
        preparation_workers: workers,
        max_non_natural_episode_fraction: 0.0,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        line_b: None,
        e_supported: None,
        soft_q: None,
        search_targets: None,
        record_process_cpu: false,
        output_directory,
    }
}

fn read_value(pin: &Value) -> Value {
    let pin: PinnedFileV1 = serde_json::from_value(pin.clone()).unwrap();
    read_pinned(&pin).unwrap()
}

/// End to end, on real games with the per-forward check on: an unconverted
/// parent update (whose records carry no contract member), its conversion, a
/// refused mixed-contract collection, and a converted two-iteration run
/// through the driver, resumed after its first iteration and revalidated,
/// whose parallel collection (a current-self fork) and prepared update
/// equal a serial collection and update bit for bit. Every record carries
/// the contract; tampered rows, markers and an auxiliary update are refused;
/// the prepared opponent cache re-resolves a hit.
#[test]
fn a_converted_run_collects_updates_and_resumes_under_the_per_forward_check() {
    on_large_stack(converted_run_end_to_end_v1);
}

fn converted_run_end_to_end_v1() {
    std::env::set_var("MTG_KERNEL_NO_STATE_DIGEST_CHECK", "1");
    let root = temp_root("run");
    let base = fresh_source(&root);

    // An unconverted parent update: real moments, no contract member.
    let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
        source: base.clone(),
        episodes: vec![episode("parent-0", 2_026_102_301, None)],
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        output_directory: root.join("parent-collect"),
    })
    .unwrap();
    let trajectories: Vec<PinnedFileV1> =
        serde_json::from_value(collection["trajectories"].clone()).unwrap();
    let update = execute_v1(ExpandedTrainingCommandV1::Update {
        source: base.clone(),
        trajectories: trajectories.clone(),
        learning_rate: 0.0003,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        output_directory: root.join("parent-update"),
    })
    .unwrap();
    for path in [
        root.join("parent-collect/collection.json"),
        trajectories[0].path.clone(),
        root.join("parent-update/update.json"),
        root.join("parent-update/checkpoint.json"),
    ] {
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("input_contract"), "{}", path.display());
    }
    let parent = ExpandedModelSourceV1 {
        checkpoint: Some(serde_json::from_value(update["checkpoint"].clone()).unwrap()),
        ..base.clone()
    };
    let (outputs, converted) = convert(&root, &parent, "conversion");
    let converted_digest = outputs.record.converted.feature_contract_digest.clone();
    let record_value = serde_json::to_value(InputContractRecordV1::of_conversion_v1(
        outputs.conversion.clone(),
    ))
    .unwrap();

    // Mixed learner and opponent contracts are refused.
    let error = execute_v1(ExpandedTrainingCommandV1::Collect {
        source: converted.clone(),
        episodes: vec![episode("mixed", 2_026_102_302, Some(parent.clone()))],
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        output_directory: root.join("mixed"),
    })
    .unwrap_err();
    assert_eq!(
        error,
        "learner and opponent use different no-state-digest contracts"
    );

    // The converted run: parallel collection and prepared updates, resumed
    // after its first iteration, then revalidated as a completed prefix.
    let parallel = run_config(&converted, 2, root.join("run-parallel"));
    let first = run_native_expanded_training_v1(&parallel, Some(1)).unwrap();
    assert_eq!(first["completed_iterations"], 1);
    let finished = run_native_expanded_training_v1(&parallel, None).unwrap();
    assert_eq!(finished["complete"], true);
    let revalidated = run_native_expanded_training_v1(&parallel, None).unwrap();
    assert_eq!(revalidated, finished);
    // The serial collection and update reach the same state and model; only
    // the checkpoint files differ, by the trajectory paths they name.
    let serial = run_config(&converted, 1, root.join("run-serial"));
    let serial_result = run_native_expanded_training_v1(&serial, None).unwrap();
    for field in ["state_sha256", "model", "adam_step"] {
        assert_eq!(
            serial_result["actual_identity"][field],
            finished["actual_identity"][field]
        );
    }
    assert_eq!(finished["actual_identity"]["adam_step"], 1 + 2);

    // Every record carries the contract.
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("run-parallel/run.json")).unwrap()).unwrap();
    assert_eq!(manifest["input_contract"], record_value);
    assert_eq!(
        manifest["feature_contract_digest"],
        converted_digest.as_str()
    );
    assert_eq!(
        manifest["feature_encoding_digest"],
        FRESH_FEATURE_IDENTITY_V4.feature_encoding_digest
    );
    let mut converted_trajectory = None;
    for receipt in finished["iterations"].as_array().unwrap() {
        let receipt = read_value(receipt);
        let collection = read_value(&receipt["collection"]);
        assert_eq!(collection["input_contract"], record_value);
        for pin in collection["trajectories"].as_array().unwrap() {
            let pin: PinnedFileV1 = serde_json::from_value(pin.clone()).unwrap();
            let trajectory: ExpandedTrajectoryV1 = read_pinned(&pin).unwrap();
            assert_eq!(trajectory.feature_contract_digest, converted_digest);
            assert_eq!(
                serde_json::to_value(trajectory.input_contract.as_ref().unwrap()).unwrap(),
                record_value
            );
            assert_eq!(
                trajectory_generation_v1(&trajectory),
                FreshLineageGenerationV1::V4
            );
            converted_trajectory = Some(trajectory);
        }
        let update = read_value(&receipt["update"]);
        assert_eq!(update["input_contract"], record_value);
        let checkpoint: ExpandedCheckpointV1 = read_pinned(
            &serde_json::from_value::<PinnedFileV1>(update["checkpoint"].clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(checkpoint.feature_contract_digest, converted_digest);
        assert_eq!(
            serde_json::to_value(checkpoint.input_contract.as_ref().unwrap()).unwrap(),
            record_value
        );
    }

    // Tampered rows and markers are refused by validation.
    let trajectory = converted_trajectory.unwrap();
    validate_trajectory(&trajectory).unwrap();
    let mut document = serde_json::to_value(&trajectory).unwrap();
    document["input_contract"] = Value::Null;
    assert!(serde_json::from_value::<ExpandedTrajectoryV1>(document.clone()).is_err());
    document.as_object_mut().unwrap().remove("input_contract");
    assert!(serde_json::from_value::<ExpandedTrajectoryV1>(document).is_ok());
    for bits in [1_u32, 0x8000_0000] {
        let mut tampered = trajectory.clone();
        tampered.decisions[0].tensor.state[150] = bits;
        assert_eq!(
            validate_trajectory(&tampered).unwrap_err(),
            "no-state-digest contract: a stored state-digest input is not +0.0"
        );
    }
    let mut unmarked = trajectory.clone();
    unmarked.input_contract = None;
    assert!(validate_trajectory(&unmarked).is_err());
    let mut relabelled = trajectory.clone();
    relabelled.feature_contract_digest = FRESH_FEATURE_IDENTITY_V4.feature_contract_digest.into();
    assert!(validate_trajectory(&relabelled).is_err());

    // The replay boundary: a stored row reproduces under the contract; -0.0
    // or nonzero digest inputs are refused; other stored-tensor readers and
    // diagnostic forwards keep the contract's refusal.
    // The last trajectory's opponent is the run's initial (run-start
    // converted) source.
    let (mut initial, _) = initialize_any_math_v1(&converted).unwrap();
    let row = trajectory
        .decisions
        .iter()
        .find(|decision| decision.actor != trajectory.episode.learner_seat)
        .expect("the episode has opponent rows");
    let common = row.tensor.tensor();
    for (bits, admitted) in [(0_u32, true), (0x8000_0000, false), (0x3f80_0000, false)] {
        let mut tensor = common.clone();
        tensor.state[DIGEST.start + 7] = f32::from_bits(bits);
        match initial.score_replay_tensor_v1(FreshLineageGenerationV1::V4, tensor) {
            Ok(scores) => {
                assert!(admitted, "{bits:#x}");
                assert_eq!(
                    scores
                        .logits
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>(),
                    row.logits
                );
                assert_eq!(scores.value.to_bits(), row.value);
            }
            Err(error) => {
                assert!(!admitted, "{bits:#x}");
                assert_eq!(
                    error,
                    "no-state-digest contract: a stored state-digest input is not +0.0"
                );
            }
        }
    }
    let tensor = NativeFlatDecisionTensorV4 { common };
    assert!(initial.score_training_tensor_v4(&tensor).is_err());
    assert!(initial.diagnostic_forward_v4(&tensor).is_err());

    // The prepared opponent cache re-resolves a hit's contract.
    let (_, first_hit) = ordered_update_preparation::load_opponent_cached_v1(&converted).unwrap();
    let (loaded, hit) = ordered_update_preparation::load_opponent_cached_v1(&converted).unwrap();
    assert!(hit || first_hit);
    assert_eq!(
        loaded.policy.input_contract_v1(),
        Some(converted_digest.as_str())
    );
    assert_eq!(
        loaded.behavior.identity.model.feature_contract_digest,
        converted_digest
    );

    // Only the ordinary CPU update: an auxiliary (line (b)) update is refused.
    let error = execute_v1(ExpandedTrainingCommandV1::Update {
        source: converted.clone(),
        trajectories: vec![PinnedFileV1 {
            path: root.join("unused.json"),
            sha256: "0".repeat(64),
        }],
        learning_rate: 0.0003,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        line_b: Some(LineBUpdateOptionsV1 {
            optimizer_mask: OptimizerMaskV1::LineBHeadOnlyV1,
            teacher: None,
        }),
        training_math: TrainingMathV1::PlatformLibm,
        output_directory: root.join("line-b"),
    })
    .unwrap_err();
    assert_eq!(
        error,
        "the no-state-digest contract admits only the ordinary CPU update"
    );
    fs::remove_dir_all(root).unwrap();
}
