//! W5 weights-only snapshots (collab LANES/lead/W5-DESIGN.md v1.3 section 2.2,
//! ruling 2): a selectable endpoint kept as its parameters alone, about a
//! third of a full train state.
//!
//! - **Snapshot:** the full checkpoint (v1 JSON or container) with both Adam
//!   moment lists removed, encoded as a checkpoint container whose `schema`
//!   is `w5-weights-snapshot/v1:` followed by the original schema. The
//!   document keeps every other field, including `state_sha256`, the full
//!   state it was cut from. No loader accepts that schema, so a snapshot can
//!   never be resumed or mistaken for a train state.
//! - **Materialization:** for play only (H panels, rating), a v1 checkpoint
//!   with the snapshot's parameters, zero moments, the original schema and
//!   the state hash of that state, published and then loaded through the
//!   continuation loader, which must give the snapshot's parameters bit for
//!   bit. The policy is the endpoint's; only the optimizer state differs.
//!
//! **S learners** (`s_learner_v1`, XS): the input is an S checkpoint record
//! (`w5-s-learner-checkpoint/v1`). Its snapshot is S's own parameters-only
//! container (`s_transformer_v1::encode_weights_snapshot_v1`), which carries
//! the record and the state hash it was cut from and which no train-state
//! loader admits. Materialization against an S template writes an S
//! container with the snapshot's parameters, zero moments and its Adam step
//! beside the output (`<output name>.s-train-state.bin`), and at the output
//! the cut record with that container, state hash and step, then loads it
//! through the S loader, which must give exactly that state (the template
//! binds the lineage: initial state, shell and features). Any other input
//! kind is refused as before.

use super::checkpoint_container_v1::{decode_v1, encode_v1, is_container_v1, parse_checkpoint_v1};
use super::*;
use crate::native_policy_train_step_v1::NativePolicyValueTrainSnapshotV1;
use crate::s_transformer_v1::{
    decode_checkpoint_v1, decode_weights_snapshot_v1, encode_checkpoint_v1,
    encode_weights_snapshot_v1, STrainStateV1, S_ARCHITECTURE_V1,
};
use s_learner_v1::{SLearnerCheckpointV1, S_LEARNER_CHECKPOINT_SCHEMA_V1, S_LEARNER_STATE_FILE_V1};

pub const WEIGHTS_SNAPSHOT_SCHEMA_PREFIX_V1: &str = "w5-weights-snapshot/v1:";

fn parameters_sha256_v1(parameters: &[ParameterBitsV1]) -> Result<String, String> {
    Ok(hex(&Sha256::digest(
        serde_json::to_vec(parameters).map_err(err)?,
    )))
}

/// Cuts a weights-only snapshot from the pinned full checkpoint `input`.
pub fn weights_snapshot_v1(input: &PinnedFileV1, output: &Path) -> Result<Value, String> {
    let bytes = read_pinned_bytes(input)?;
    ensure(
        decode_weights_snapshot_v1(&bytes).is_err(),
        "the input is already a weights snapshot",
    )?;
    if is_s_record_v1(&bytes) {
        return s_weights_snapshot_v1(input, &bytes, output);
    }
    let mut saved = parse_checkpoint_v1(&bytes)?;
    ensure(
        !saved.schema.starts_with(WEIGHTS_SNAPSHOT_SCHEMA_PREFIX_V1),
        "the input is already a weights snapshot",
    )?;
    ensure(
        !saved.parameters.is_empty()
            && saved.first_moments.len() == saved.parameters.len()
            && saved.second_moments.len() == saved.parameters.len(),
        "a weights snapshot is cut from a full train state",
    )?;
    let parameters_sha256 = parameters_sha256_v1(&saved.parameters)?;
    saved.first_moments.clear();
    saved.second_moments.clear();
    saved.schema = format!("{WEIGHTS_SNAPSHOT_SCHEMA_PREFIX_V1}{}", saved.schema);
    let encoded = encode_v1(&saved)?;
    let back = decode_v1(&encoded)?;
    ensure(
        parameters_sha256_v1(&back.parameters)? == parameters_sha256
            && back.first_moments.is_empty()
            && back.second_moments.is_empty(),
        "the weights snapshot does not decode to its parameters",
    )?;
    let written = publish_checkpoint_file_v1(output, &encoded)?;
    Ok(
        json!({"schema":"w5-weights-snapshot-receipt/v1","input":input,"output":written,
        "cut_from_state_sha256":saved.state_sha256,"parameters_sha256":parameters_sha256,
        "adam_step":saved.adam_step,"input_bytes":bytes.len(),"snapshot_bytes":encoded.len(),
        "git_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD")}),
    )
}

/// An S checkpoint record names its trajectories and scalars only (at most
/// 1,024 trajectory pins): far below this. Larger inputs are never probed,
/// so a net-8 checkpoint's parameter arrays are parsed once, by its own
/// parser, as before.
const S_RECORD_MAX_BYTES_V1: usize = 4 << 20;

/// Whether `bytes` is an S checkpoint record (by its schema; the record's
/// own parse checks the rest).
fn is_s_record_v1(bytes: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct SchemaProbeV1 {
        schema: String,
    }
    bytes.len() <= S_RECORD_MAX_BYTES_V1
        && !is_container_v1(bytes)
        && serde_json::from_slice::<SchemaProbeV1>(bytes)
            .is_ok_and(|probe| probe.schema == S_LEARNER_CHECKPOINT_SCHEMA_V1)
}

/// The S record's full state, checked against the record.
fn s_record_state_v1(bytes: &[u8]) -> Result<(SLearnerCheckpointV1, STrainStateV1, usize), String> {
    let record: SLearnerCheckpointV1 = serde_json::from_slice(bytes).map_err(err)?;
    ensure(
        record.schema == S_LEARNER_CHECKPOINT_SCHEMA_V1 && record.architecture == S_ARCHITECTURE_V1,
        "unknown S checkpoint record schema or architecture",
    )?;
    let container = read_pinned_bytes(&record.state)?;
    let state = decode_checkpoint_v1(&container)?;
    ensure(
        state.state_sha256_v1() == record.state_sha256 && state.adam_step_v1() == record.adam_step,
        "S checkpoint record and container differ",
    )?;
    Ok((record, state, container.len()))
}

/// [`weights_snapshot_v1`] for an S checkpoint record.
fn s_weights_snapshot_v1(
    input: &PinnedFileV1,
    bytes: &[u8],
    output: &Path,
) -> Result<Value, String> {
    let (record, state, container_bytes) = s_record_state_v1(bytes)?;
    let record_value: Value = serde_json::from_slice(bytes).map_err(err)?;
    let encoded = encode_weights_snapshot_v1(&state, &record_value);
    let back = decode_weights_snapshot_v1(&encoded)?;
    let parameters_sha256 = state.model_v1().parameter_sha256_v1();
    ensure(
        back.model.parameter_sha256_v1() == parameters_sha256
            && back.cut_from_state_sha256 == record.state_sha256
            && back.adam_step == record.adam_step
            && back.record == record_value,
        "the S weights snapshot does not decode to its parameters and record",
    )?;
    let written = publish_checkpoint_file_v1(output, &encoded)?;
    Ok(
        json!({"schema":"w5-weights-snapshot-receipt/v1","input":input,"output":written,
        "learner":S_ARCHITECTURE_V1,"cut_from_state_sha256":record.state_sha256,
        "parameters_sha256":parameters_sha256,"adam_step":record.adam_step,
        "input_bytes":bytes.len() + container_bytes,"snapshot_bytes":encoded.len(),
        "git_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD")}),
    )
}

/// [`materialize_weights_snapshot_v1`] for an S template.
fn s_materialize_v1(
    template: &ExpandedModelSourceV1,
    snapshot: &PinnedFileV1,
    bytes: &[u8],
    output: &Path,
) -> Result<Value, String> {
    let snap = decode_weights_snapshot_v1(bytes)?;
    let mut record: SLearnerCheckpointV1 =
        serde_json::from_value(snap.record.clone()).map_err(err)?;
    ensure(
        record.schema == S_LEARNER_CHECKPOINT_SCHEMA_V1 && record.architecture == S_ARCHITECTURE_V1,
        "the S weights snapshot carries an unknown record",
    )?;
    // The embedded record is the one the snapshot was cut from: its state
    // hash and step are the snapshot's own, checked before any file exists.
    ensure(
        record.state_sha256 == snap.cut_from_state_sha256 && record.adam_step == snap.adam_step,
        "the S weights snapshot's record names another state or step",
    )?;
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("output has no file name")?;
    let container_path = output.with_file_name(format!("{name}.{S_LEARNER_STATE_FILE_V1}"));
    ensure(
        output.is_absolute() && !output.exists() && !container_path.exists(),
        "a fresh absolute output path is required",
    )?;
    let total = snap.model.layout_v1().total;
    let state = STrainStateV1::from_parts_v1(
        snap.model.clone(),
        vec![0.0; total],
        vec![0.0; total],
        snap.adam_step,
    )?;
    let state_sha256 = state.state_sha256_v1();
    let parameters_sha256 = snap.model.parameter_sha256_v1();
    record.state = publish_checkpoint_file_v1(&container_path, &encode_checkpoint_v1(&state))?;
    record.state_sha256 = state_sha256.clone();
    record.adam_step = snap.adam_step;
    let written = publish_checkpoint_file_v1(output, &serde_json::to_vec(&record).map_err(err)?)?;
    let materialized = ExpandedModelSourceV1 {
        checkpoint: Some(written.clone()),
        ..template.clone()
    };
    let loaded = s_learner_v1::load_s_v1(&materialized, None)?;
    ensure(
        loaded.state == state && loaded.state.model_v1().parameter_sha256_v1() == parameters_sha256,
        "the materialized S checkpoint does not load the snapshot's parameters",
    )?;
    Ok(
        json!({"schema":"w5-weights-materialization/v1","snapshot":snapshot,"output":written,
        "state":record.state,"learner":S_ARCHITECTURE_V1,"template":template,
        "parameters_sha256":parameters_sha256,"cut_from_state_sha256":snap.cut_from_state_sha256,
        "materialized_state_sha256":state_sha256,"moments":"zero",
        "git_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD")}),
    )
}

/// The document of a pinned S train-state container (an S learner's
/// `initial` or a record's `state`), after its full decode: schema,
/// architecture, config, Adam step, state hash, and the parameter names and
/// shapes in layout order. Python tools read S layouts through this, never by
/// parsing a container.
pub fn s_container_document_v1(input: &PinnedFileV1) -> Result<Value, String> {
    let state = decode_checkpoint_v1(&read_pinned_bytes(input)?)?;
    let model = state.model_v1();
    let parameters: Vec<Value> = model
        .layout_v1()
        .tensors
        .iter()
        .map(|spec| json!({"name":spec.name,"shape":spec.shape}))
        .collect();
    Ok(
        json!({"schema":crate::s_transformer_v1::S_TRAIN_STATE_SCHEMA_V1,
        "architecture":S_ARCHITECTURE_V1,"config":model.config_v1(),
        "adam_step":state.adam_step_v1(),"state_sha256":state.state_sha256_v1(),
        "parameters_sha256":model.parameter_sha256_v1(),"parameters":parameters}),
    )
}

/// A playable v1 checkpoint from the pinned weights snapshot: its
/// parameters, zero moments, its original schema and the state hash of that
/// state. `template` is a source of the same lineage (its play import and
/// feature transfer, and any checkpoint or none) that supplies the model's
/// parameter names and shapes.
pub fn materialize_weights_snapshot_v1(
    template: &ExpandedModelSourceV1,
    snapshot: &PinnedFileV1,
    output: &Path,
) -> Result<Value, String> {
    let bytes = read_pinned_bytes(snapshot)?;
    ensure(
        is_container_v1(&bytes),
        "a weights snapshot is a checkpoint container",
    )?;
    if template.s_learner.is_some() {
        return s_materialize_v1(template, snapshot, &bytes, output);
    }
    let mut saved = decode_v1(&bytes)?;
    let schema = saved
        .schema
        .strip_prefix(WEIGHTS_SNAPSHOT_SCHEMA_PREFIX_V1)
        .ok_or("the input is not a weights snapshot")?
        .to_owned();
    ensure(
        saved.first_moments.is_empty() && saved.second_moments.is_empty(),
        "a weights snapshot carries no moments",
    )?;
    let parameters_sha256 = parameters_sha256_v1(&saved.parameters)?;
    let cut_from = saved.state_sha256.clone();
    let (_, state) = initialize_any_math_v1(template)?;
    let names = state.model_v1().parameter_snapshot_v1();
    let parameters = restore_parameters(&saved.parameters, &names)?;
    let zeros: Vec<NativeNamedParameterV1> = names
        .iter()
        .map(|t| NativeNamedParameterV1 {
            name: t.name,
            shape: t.shape.clone(),
            values: vec![0.0; t.values.len()],
        })
        .collect();
    let snap = NativePolicyValueTrainSnapshotV1 {
        adam_step: saved.adam_step,
        scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
        parameters,
        first_moments: zeros.clone(),
        second_moments: zeros.clone(),
    };
    let state_sha256 = hex(&snap.state_sha256_v1().map_err(err)?);
    let zero_bits: Vec<ParameterBitsV1> = zeros.iter().map(ParameterBitsV1::from_native).collect();
    saved.schema = schema;
    saved.first_moments = zero_bits.clone();
    saved.second_moments = zero_bits;
    saved.state_sha256 = state_sha256.clone();
    let v1 = serde_json::to_vec(&saved).map_err(err)?;
    let written = publish_checkpoint_file_v1(output, &v1)?;
    let materialized = ExpandedModelSourceV1 {
        checkpoint: Some(written.clone()),
        ..template.clone()
    };
    let (_, loaded) = initialize_any_math_v1(&materialized)?;
    let loaded_parameters: Vec<ParameterBitsV1> = loaded
        .model_v1()
        .parameter_snapshot_v1()
        .iter()
        .map(ParameterBitsV1::from_native)
        .collect();
    ensure(
        parameters_sha256_v1(&loaded_parameters)? == parameters_sha256,
        "the materialized checkpoint does not load the snapshot's parameters",
    )?;
    Ok(
        json!({"schema":"w5-weights-materialization/v1","snapshot":snapshot,"output":written,
        "template":template,"parameters_sha256":parameters_sha256,"cut_from_state_sha256":cut_from,
        "materialized_state_sha256":state_sha256,"moments":"zero",
        "git_commit":env!("MTG_KERNEL_BUILD_GIT_HEAD")}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;

    fn list(id: &str) -> ExpandedDeckListV1 {
        let d = checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
        let c = d.registered_configuration();
        ExpandedDeckListV1 {
            label: id.into(),
            mainboard: c.mainboard().to_vec(),
            sideboard: c.sideboard().to_vec(),
        }
    }

    /// A real update's checkpoint: the snapshot is about a third of it and
    /// refuses to load as a train state; its materialization loads the same
    /// parameters with zero moments; a snapshot of a snapshot is refused.
    #[test]
    fn snapshot_keeps_parameters_and_materializes_a_playable_checkpoint() {
        let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
        let root = std::env::temp_dir().join(format!("w5-weights-snapshot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let descriptor =
            fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
                &root.join("source"),
                feature_identity,
                |_parameters| {},
            );
        let descriptor_path = root.join("descriptor.json");
        let bytes = serde_json::to_vec(&descriptor).unwrap();
        fs::write(&descriptor_path, &bytes).unwrap();
        let source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: descriptor_path.canonicalize().unwrap(),
                sha256: hex(&Sha256::digest(bytes)),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
                expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
            },
            checkpoint: None,
            s_learner: None,
        };
        let decks = [list("Affinity"), list("Terror")];
        let episode = ExpandedEpisodeV1 {
            id: "w5-weights-snapshot".into(),
            seed: 2026110301,
            starting_player: 0,
            learner_seat: 0,
            opponent: Some(source.clone()),
            opponent_search: None,
            opponent_kind: None,
            registered: decks.clone(),
            selected: decks,
            postboard: false,
            max_physical_decisions: 100000,
            max_policy_steps: 1000000,
            runtime_rules: None,
            reset: None,
            opening: None,
            mulligan_modes: None,
        };
        let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
            source: source.clone(),
            episodes: vec![episode],
            max_non_natural_episode_fraction: 0.0,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            output_directory: root.join("collect"),
        })
        .unwrap();
        let trajectories: Vec<PinnedFileV1> =
            serde_json::from_value(collection["trajectories"].clone()).unwrap();
        let update = execute_v1(ExpandedTrainingCommandV1::Update {
            source: source.clone(),
            trajectories,
            learning_rate: 0.0001,
            value_coefficient: 0.5,
            update_backend: ExpandedUpdateBackendV1::Cpu,
            update_backward_execution: UpdateBackwardExecutionV1::Sequential,
            loss_selection: ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                gamma: 1.0,
                lambda: 0.9,
                entropy_coefficient: 0.0,
            },
            line_b: None,
            training_math: TrainingMathV1::PlatformLibm,
            record_format: CheckpointRecordFormatV1::V2,
            output_directory: root.join("update"),
        })
        .unwrap();
        let full: PinnedFileV1 = serde_json::from_value(update["checkpoint"].clone()).unwrap();
        let receipt = weights_snapshot_v1(&full, &root.join("snapshot.bin")).unwrap();
        assert_eq!(
            receipt["cut_from_state_sha256"],
            update["after_state_sha256"]
        );
        let snapshot: PinnedFileV1 = serde_json::from_value(receipt["output"].clone()).unwrap();
        let full_bytes = fs::metadata(&full.path).unwrap().len();
        let snapshot_bytes = fs::metadata(&snapshot.path).unwrap().len();
        assert!(
            snapshot_bytes * 5 < full_bytes * 2,
            "snapshot {snapshot_bytes} bytes against a {full_bytes}-byte full state"
        );
        // Never a train state.
        let as_source = ExpandedModelSourceV1 {
            checkpoint: Some(snapshot.clone()),
            ..source.clone()
        };
        assert!(initialize_any_math_v1(&as_source).is_err());
        assert!(weights_snapshot_v1(&snapshot, &root.join("again.bin"))
            .unwrap_err()
            .contains("already a weights snapshot"));
        // Materialized: the endpoint's parameters, zero moments.
        let made =
            materialize_weights_snapshot_v1(&source, &snapshot, &root.join("materialized.json"))
                .unwrap();
        assert_eq!(made["parameters_sha256"], receipt["parameters_sha256"]);
        assert_ne!(
            made["materialized_state_sha256"],
            update["after_state_sha256"]
        );
        let pin: PinnedFileV1 = serde_json::from_value(made["output"].clone()).unwrap();
        let (_, original) = initialize_any_math_v1(&ExpandedModelSourceV1 {
            checkpoint: Some(full),
            ..source.clone()
        })
        .unwrap();
        let (_, loaded) = initialize_any_math_v1(&ExpandedModelSourceV1 {
            checkpoint: Some(pin),
            ..source.clone()
        })
        .unwrap();
        let bits = |state: &NativePolicyValueTrainStateV1| -> Vec<ParameterBitsV1> {
            state
                .model_v1()
                .parameter_snapshot_v1()
                .iter()
                .map(ParameterBitsV1::from_native)
                .collect()
        };
        assert_eq!(bits(&original), bits(&loaded));
        let snap = loaded.snapshot_v1().unwrap();
        assert!(snap
            .first_moments
            .iter()
            .chain(&snap.second_moments)
            .all(|p| p.values.iter().all(|v| v.to_bits() == 0)));
        assert!(
            materialize_weights_snapshot_v1(&source, &snapshot, &root.join("materialized.json"))
                .is_err(),
            "a fresh output path is required"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
