//! Forward-only scoring of pinned public tensors. Never samples or steps a game.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedStateProbeV1 {
    pub probe_id: String,
    pub trajectory: PinnedFileV1,
    pub row_index: usize,
}

pub(super) fn score_v1(
    source: ExpandedModelSourceV1,
    probes: Vec<SavedStateProbeV1>,
    membership_sha256: String,
    output_directory: PathBuf,
) -> Result<Value, String> {
    ensure(
        !probes.is_empty() && probes.len() <= 3072,
        "saved-state probe count outside1..3072",
    )?;
    ensure(
        membership_sha256.len() == 64 && membership_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid probe membership sha256",
    )?;
    ensure(
        !output_directory.exists(),
        "probe output directory must be fresh",
    )?;
    let (policy, state) = initialize(&source)?;
    let generation = policy.feature_identity_v1().generation;
    let mut ids = BTreeSet::new();
    let mut by_trajectory = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for (i, probe) in probes.iter().enumerate() {
        ensure(ids.insert(&probe.probe_id), "duplicate probe id")?;
        by_trajectory
            .entry(probe.trajectory.sha256.clone())
            .or_default()
            .push(i);
    }
    let mut rows = vec![Value::Null; probes.len()];
    for indices in by_trajectory.values() {
        let pin = &probes[indices[0]].trajectory;
        ensure(
            fs::metadata(&pin.path).map_err(err)?.len() <= MAX_BATCH_BYTES,
            "probe trajectory exceeds ingestion bound",
        )?;
        let trajectory: ExpandedTrajectoryV1 = read_pinned(pin)?;
        identity_valid(
            &trajectory.feature_contract_digest,
            &trajectory.feature_encoding_digest,
            &trajectory.card_db_hash,
        )?;
        ensure(
            trajectory_generation_v1(&trajectory) == generation,
            "probe feature generation differs from model",
        )?;
        for &i in indices {
            let probe = &probes[i];
            ensure(
                probe.trajectory == *pin,
                "same trajectory hash with inconsistent source path",
            )?;
            let row = trajectory
                .decisions
                .get(probe.row_index)
                .ok_or("probe row index out of range")?;
            let tensor = row.tensor.tensor();
            let view = encoded_decision_view_generic_v1(&tensor, generation);
            let output = match generation {
                FreshLineageGenerationV1::V3 => state.model_v1().forward_feature_transfer_v3(view),
                FreshLineageGenerationV1::V4 => state.model_v1().forward_feature_transfer_v4(view),
            }
            .map_err(err)?;
            let (_, lp) =
                crate::native_policy_train_step_v1::selected_log_softmax(&output.logits, 0)
                    .map_err(err)?;
            let probabilities: Vec<f32> = lp.iter().map(|p| p.exp()).collect();
            rows[i] = json!({"probe_id":probe.probe_id,"trajectory":probe.trajectory,"row_index":probe.row_index,
                "logits_bits":bits(&output.logits),"trainer_log_probabilities_bits":bits(&lp),
                "trainer_probabilities_bits":bits(&probabilities),"value_bits":output.value.to_bits()});
        }
    }
    let result = json!({"schema":"native-saved-state-probe/v1","complete":true,"source":source,
        "membership_sha256":membership_sha256,"state_sha256":hex(&state.state_sha256_v1().map_err(err)?),
        "adam_step":state.adam_step_v1(),"forward_calls":probes.len(),"rows":rows});
    fs::create_dir_all(&output_directory).map_err(err)?;
    publish_json(&output_directory, "probes.json", &result)?;
    Ok(result)
}
