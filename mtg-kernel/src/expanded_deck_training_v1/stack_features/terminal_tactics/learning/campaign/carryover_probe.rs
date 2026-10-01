//! Fixed-input, one-step optimizer diagnosis. No checkpoint output or rollout.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    source: ExpandedModelSourceV1,
    dataset: PinnedFileV1,
    retention_dataset: PinnedFileV1,
    first_checkpoint: PinnedFileV1,
    output_directory: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedFirst {
    checkpoint: Checkpoint,
    trajectory: Option<PinnedFileV1>,
    retention_dataset: Option<PinnedFileV1>,
    selected_rows: Vec<Vec<usize>>,
}

fn same<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(serde_json::to_value(a).map_err(err)? == serde_json::to_value(b).map_err(err)?)
}

pub(super) fn run(c: Command) -> Result<Value, String> {
    let data: Value = serde_json::from_slice(&read_pinned_bytes(&c.dataset)?).map_err(err)?;
    ensure(
        data["schema"] == "public-terminal-teacher-data/v1"
            && data["source"] == serde_json::to_value(&c.source).map_err(err)?,
        "carryover teacher identity differs",
    )?;
    let rows = data["records"]
        .as_array()
        .ok_or("carryover teacher records missing")?;
    ensure(rows.len() == 32, "carryover requires32 teaching inputs")?;
    let mut ids = BTreeSet::new();
    for row in rows {
        ensure(
            row["spec"]["split"] == "train"
                && row["spec"]["no_win_control"] == false
                && ids.insert(row["id"].as_str().ok_or("carryover teacher id missing")?),
            "carryover rejects heldout, targetless or duplicate teacher",
        )?;
        let winners = row["data"]["winning_indices"]
            .as_array()
            .ok_or("carryover winners missing")?;
        ensure(
            winners.len() == 1,
            "carryover requires one natural winning witness",
        )?;
        let winner = winners[0].as_u64().ok_or("carryover winner invalid")? as usize;
        let outcomes = row["data"]["outcomes"]
            .as_array()
            .ok_or("carryover outcomes missing")?;
        ensure(
            winner < outcomes.len()
                && outcomes[winner]["classification"] == "win"
                && outcomes[winner]["terminal"]["terminal_classification"] == "natural"
                && outcomes[winner]["terminal"]["winner"]
                    == row["data"]["visible"]["acting_player"],
            "carryover target lacks natural witness",
        )?;
    }
    let (mut policy, original) = initialize(&c.source)?;
    ensure(
        original.adam_step_v1() == 32400
            && policy.feature_identity_v1().generation == FreshLineageGenerationV1::V4
            && data["model"]
                == serde_json::to_value(policy.actual_model_identity_v1()).map_err(err)?,
        "carryover requires original V4 g115 parent",
    )?;
    let original_hash = hex(&original.state_sha256_v1().map_err(err)?);
    let retained =
        super::retention_data::load(&c.source, &c.retention_dataset, &c.dataset, &policy)?;
    ensure(
        retained.groups.iter().map(Vec::len).sum::<usize>() == 303,
        "carryover requires303 retention rows",
    )?;
    let saved: SavedFirst =
        serde_json::from_slice(&read_pinned_bytes(&c.first_checkpoint)?).map_err(err)?;
    let s = &saved.checkpoint;
    ensure(
        s.schema == "terminal-retained-engineering/v1"
            && s.loss_identity == "terminal-ce-plus-parent-forward-kl-engineering-beta-half/v1"
            && same(&s.source, &c.source)?
            && same(&s.dataset, &c.dataset)?
            && saved.trajectory.is_none()
            && same(&saved.retention_dataset, &Some(c.retention_dataset.clone()))?
            && saved.selected_rows == retained.selected_rows
            && !s.target_permuted
            && s.backend == "cpu"
            && s.device_ordinal == 0
            && s.initial_adam == 32400
            && s.completed_updates == 1
            && s.adam_step == 32401,
        "carryover first checkpoint ancestry or age differs",
    )?;
    let template = original.snapshot_v1().map_err(err)?;
    let restored = NativePolicyValueTrainSnapshotV1 {
        parameters: restore_parameters(&s.parameters, &template.parameters)?,
        first_moments: restore_parameters(&s.first_moments, &template.first_moments)?,
        second_moments: restore_parameters(&s.second_moments, &template.second_moments)?,
        adam_step: s.adam_step,
        scorer_bias_anchor_bits: s.scorer_bias_anchor_bits,
    };
    ensure(
        hex(&restored.state_sha256_v1().map_err(err)?) == s.state_sha256,
        "carryover saved state hash differs",
    )?;
    let actual =
        NativePolicyValueTrainStateV1::from_snapshot_v1(original.model_v1().clone(), &restored)
            .map_err(err)?;
    let momentum = original
        .diagnostic_zero_gradient_successor_v1()
        .map_err(err)?;
    let mut inputs = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let t: TensorBitsV1 = serde_json::from_value(row["data"]["tensor"].clone()).map_err(err)?;
        let t = NativeFlatDecisionTensorV4 { common: t.tensor() };
        ensure(
            policy.score_training_tensor_v4(&t)?.logits.len()
                == row["data"]["outcomes"].as_array().unwrap().len(),
            "carryover teacher action width differs",
        )?;
        inputs.push(("teacher", index, 0, t));
    }
    for (g, group) in retained.groups.into_iter().enumerate() {
        for (r, (t, _)) in group.into_iter().enumerate() {
            inputs.push(("retention", g, r, t));
        }
    }
    let mut models = Vec::new();
    let mut sampler = WideCategoricalScratchV1::default();
    for (label, state) in [
        ("parent", &original),
        ("momentum_only", &momentum),
        ("actual_first_update", &actual),
    ] {
        let snapshot = state.snapshot_v1().map_err(err)?;
        policy.replace_training_parameters_v3(&snapshot.parameters)?;
        let mut scored = Vec::new();
        for (split, group, row, tensor) in &inputs {
            let score = policy.score_training_tensor_v4(tensor)?;
            let masses = sampler.apportion(&score.logits).map_err(err)?;
            ensure(
                masses.iter().sum::<u128>() == (1u128 << 64),
                "carryover masses invalid",
            )?;
            scored.push(json!({"split":split,"group":group,"row":row,"logits_bits":bits(&score.logits),
                "value_bits":score.value.to_bits(),"masses":masses.iter().map(u128::to_string).collect::<Vec<_>>() }));
        }
        models.push(json!({"label":label,"state_sha256":hex(&state.state_sha256_v1().map_err(err)?),"adam_step":state.adam_step_v1(),"records":scored}));
    }
    let snapshot = momentum.snapshot_v1().map_err(err)?;
    let mut tensor_hashes = Vec::new();
    for (section, tensors) in [
        ("parameters", &snapshot.parameters),
        ("first_moments", &snapshot.first_moments),
        ("second_moments", &snapshot.second_moments),
    ] {
        for tensor in tensors {
            let raw: Vec<u8> = tensor
                .values
                .iter()
                .flat_map(|v| v.to_bits().to_le_bytes())
                .collect();
            tensor_hashes.push(json!({"section":section,"name":tensor.name,"shape":tensor.shape,"f32_bits_le_sha256":sha(&raw)}));
        }
    }
    ensure(
        original_hash == hex(&original.state_sha256_v1().map_err(err)?),
        "carryover mutated original",
    )?;
    let result = json!({"schema":"terminal-optimizer-carryover-probe/v1","complete":true,"source":c.source,
        "dataset":c.dataset,"retention_dataset":c.retention_dataset,"first_checkpoint":c.first_checkpoint,
        "positions":335,"counterfactual_adam_steps":1,"training_updates":0,"models":models,
        "momentum_tensor_hashes":tensor_hashes,"original_state_unchanged":true,
        "non_claim":"Fixed consumed inputs and one in-memory zero-gradient step; no causal BO3 attribution, model publication or playing-strength evidence."});
    fs::create_dir(&c.output_directory).map_err(err)?;
    publish_json(&c.output_directory, "result.json", &result)?;
    Ok(result)
}
