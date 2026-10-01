//! At most two CPU updates to qualify real-checkpoint retention and resume.
//! This command cannot run a substantive campaign or choose a retention weight.
use super::*;
use crate::native_policy_train_step_v1::retention_v1::{RetentionGroupV1, RetentionRowV1};

const RETAINED_SCHEMA: &str = "terminal-retained-engineering/v1";
const RETAINED_LOSS: &str = "terminal-ce-plus-parent-forward-kl-engineering-beta-half/v1";
const BETA: f32 = 0.5;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    source: ExpandedModelSourceV1,
    dataset: PinnedFileV1,
    trajectory: Option<PinnedFileV1>,
    retention_dataset: Option<PinnedFileV1>,
    output_directory: PathBuf,
    workers: usize,
    end_update: usize,
    resume: Option<PinnedFileV1>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    checkpoint: Checkpoint,
    trajectory: Option<PinnedFileV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retention_dataset: Option<PinnedFileV1>,
    selected_rows: Vec<Vec<usize>>,
}

fn same<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(serde_json::to_value(a).map_err(err)? == serde_json::to_value(b).map_err(err)?)
}

pub(super) fn run(c: Command) -> Result<Value, String> {
    ensure(
        matches!(c.workers, 1 | 4) && (1..=2).contains(&c.end_update),
        "retained engineering requires 1/4 workers and at most two updates",
    )?;
    ensure(
        c.trajectory.is_some() != c.retention_dataset.is_some(),
        "retained engineering needs exactly one retention input",
    )?;
    let data: Value = serde_json::from_slice(&read_pinned_bytes(&c.dataset)?).map_err(err)?;
    ensure(
        data["schema"] == "public-terminal-teacher-data/v1"
            && data["source"] == serde_json::to_value(&c.source).map_err(err)?,
        "retained teacher identity differs",
    )?;
    let rows = data["records"]
        .as_array()
        .ok_or("missing retained teacher rows")?;
    ensure(
        rows.len() == 32,
        "retained engineering requires32 training rows",
    )?;
    let (parent, original) = initialize(&c.source)?;
    ensure(
        data["model"] == serde_json::to_value(parent.actual_model_identity_v1()).map_err(err)?
            && parent.feature_identity_v1().generation == FreshLineageGenerationV1::V4,
        "retained teacher model differs",
    )?;
    let initial_adam = original.adam_step_v1();
    let original_hash = hex(&original.state_sha256_v1().map_err(err)?);
    let mut teaching = Vec::new();
    let mut targets = Vec::new();
    let mut teacher_tensors = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for row in rows {
        ensure(
            row["spec"]["split"] == "train"
                && row["spec"]["no_win_control"] == false
                && ids.insert(row["id"].as_str().ok_or("missing teacher id")?),
            "retained engineering rejects heldout/duplicate/targetless teacher",
        )?;
        let winners = row["data"]["winning_indices"]
            .as_array()
            .ok_or("missing winners")?;
        ensure(winners.len() == 1, "retained teacher needs one winner")?;
        let winner = winners[0].as_u64().ok_or("invalid winner")? as usize;
        let outcomes = row["data"]["outcomes"]
            .as_array()
            .ok_or("missing outcomes")?;
        ensure(
            winner < outcomes.len()
                && outcomes[winner]["classification"] == "win"
                && outcomes[winner]["terminal"]["terminal_classification"] == "natural"
                && outcomes[winner]["terminal"]["winner"]
                    == row["data"]["visible"]["acting_player"],
            "retained target lacks natural winning witness",
        )?;
        let t: TensorBitsV1 = serde_json::from_value(row["data"]["tensor"].clone()).map_err(err)?;
        teacher_tensors.insert(sha(&serde_json::to_vec(&t).map_err(err)?));
        let t = NativeFlatDecisionTensorV4 { common: t.tensor() };
        ensure(
            parent.score_training_tensor_v4(&t)?.logits.len() == outcomes.len(),
            "retained teacher menu differs",
        )?;
        teaching.push(t);
        targets.push(winner);
    }
    let (retained, selected, exact, parent_score_replay_rows) = if let Some(pin) =
        &c.retention_dataset
    {
        let loaded = super::retention_data::load(&c.source, pin, &c.dataset, &parent)?;
        let count = loaded.groups.iter().map(Vec::len).sum::<usize>();
        (loaded.groups, loaded.selected_rows, 0usize, count)
    } else {
        let archive: Trajectory = serde_json::from_slice(&read_pinned_bytes(
            c.trajectory.as_ref().ok_or("missing retention archive")?,
        )?)
        .map_err(err)?;
        ensure(
            archive.schema == "mtg-kernel-public-stack-trajectory/v1"
                && archive.decisions.len() == archive.auxiliary.len()
                && archive.episode.learner_seat < 2
                && same(
                    archive
                        .episode
                        .opponent
                        .as_ref()
                        .ok_or("retained archive lacks opponent")?,
                    &c.source,
                )?,
            "retained archive requires original parent opponent",
        )?;
        // Select the first eight complete physical groups from each actor without
        // examining scores, selected actions or terminal outcomes. Preserve every
        // substep; this is an engineering fixture, not a representative dataset.
        let mut counts = [0usize; 2];
        let mut selected = Vec::new();
        let mut index = 0;
        let mut physical_ids = BTreeSet::new();
        while index < archive.decisions.len() {
            let first = &archive.decisions[index];
            let count = first.substep_count as usize;
            ensure(
                first.actor < 2
                    && first.substep_index == 0
                    && count > 0
                    && count <= 64
                    && index + count <= archive.decisions.len()
                    && physical_ids.insert((first.actor, first.physical_decision_id)),
                "invalid retained physical group",
            )?;
            for (offset, row) in archive.decisions[index..index + count].iter().enumerate() {
                ensure(
                    row.actor == first.actor
                        && row.physical_decision_id == first.physical_decision_id
                        && row.substep_index as usize == offset
                        && row.substep_count as usize == count,
                    "retained physical substeps differ",
                )?;
            }
            if counts[first.actor as usize] < 8 {
                selected.push((index..index + count).collect::<Vec<_>>());
                counts[first.actor as usize] += 1;
            }
            index += count;
        }
        ensure(
            counts == [8, 8],
            "retained fixture requires eight physical groups per actor",
        )?;
        let mut retained = Vec::new();
        let mut exact = 0;
        for group in &selected {
            let mut capture = Vec::new();
            for &i in group {
                let row = &archive.decisions[i];
                ensure(
                    !teacher_tensors.contains(&sha(&serde_json::to_vec(&row.tensor).map_err(err)?)),
                    "teacher tensor overlaps retention",
                )?;
                let tensor = NativeFlatDecisionTensorV4 {
                    common: row.tensor.tensor(),
                };
                let score = parent.score_training_tensor_v4(&tensor)?;
                ensure(
                    score.logits.len() == row.logits.len(),
                    "retained archive menu differs",
                )?;
                if row.actor != archive.episode.learner_seat {
                    ensure(
                        bits(&score.logits) == row.logits && score.value.to_bits() == row.value,
                        "retained parent behavior replay differs",
                    )?;
                    exact += 1;
                }
                capture.push((tensor, score.logits));
            }
            retained.push(capture);
        }
        ensure(exact > 0, "retained fixture lacks parent replay")?;
        let count = retained.iter().map(Vec::len).sum::<usize>();
        (retained, selected, exact, count)
    };
    let mut state = original.clone();
    let mut completed = 0;
    if let Some(pin) = &c.resume {
        let saved: Saved = serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        let s = &saved.checkpoint;
        ensure(
            s.schema == RETAINED_SCHEMA
                && s.loss_identity == RETAINED_LOSS
                && same(&s.source, &c.source)?
                && same(&s.dataset, &c.dataset)?
                && same(&saved.trajectory, &c.trajectory)?
                && same(&saved.retention_dataset, &c.retention_dataset)?
                && saved.selected_rows == selected
                && !s.target_permuted
                && s.backend == "cpu"
                && s.device_ordinal == 0
                && s.initial_adam == initial_adam
                && s.completed_updates == 1
                && s.adam_step == initial_adam + 1,
            "retained resume contract differs",
        )?;
        let template = state.snapshot_v1().map_err(err)?;
        let snapshot = NativePolicyValueTrainSnapshotV1 {
            parameters: restore_parameters(&s.parameters, &template.parameters)?,
            first_moments: restore_parameters(&s.first_moments, &template.first_moments)?,
            second_moments: restore_parameters(&s.second_moments, &template.second_moments)?,
            adam_step: s.adam_step,
            scorer_bias_anchor_bits: s.scorer_bias_anchor_bits,
        };
        ensure(
            hex(&snapshot.state_sha256_v1().map_err(err)?) == s.state_sha256,
            "retained resume state hash differs",
        )?;
        state =
            NativePolicyValueTrainStateV1::from_snapshot_v1(state.model_v1().clone(), &snapshot)
                .map_err(err)?;
        completed = 1;
    }
    ensure(
        completed < c.end_update,
        "retained endpoint already reached",
    )?;
    let mut policy = parent;
    policy.replace_training_parameters_v3(&state.snapshot_v1().map_err(err)?.parameters)?;
    fs::create_dir(&c.output_directory).map_err(err)?;
    let started = std::time::Instant::now();
    let mut updates = Vec::new();
    while completed < c.end_update {
        let scores = teaching
            .iter()
            .map(|t| policy.score_training_tensor_v4(t))
            .collect::<Result<Vec<_>, _>>()?;
        let logits: Vec<_> = scores.iter().map(|s| bits(&s.logits)).collect();
        let steps: Vec<Vec<_>> = teaching
            .iter()
            .enumerate()
            .map(|(i, t)| {
                vec![NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        encoded_decision_view_generic_v1(&t.common, FreshLineageGenerationV1::V4),
                    )),
                    selected_action_index: targets[i],
                    expected_raw_action_logit_bits: &logits[i],
                    expected_value_bits: scores[i].value.to_bits(),
                }]
            })
            .collect();
        let groups: Vec<_> = steps
            .iter()
            .map(|substeps| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: 1,
                baseline_bits: 0,
            })
            .collect();
        let current = retained
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|(t, _)| policy.score_training_tensor_v4(t))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let replay: Vec<Vec<_>> = current
            .iter()
            .map(|group| group.iter().map(|s| bits(&s.logits)).collect())
            .collect();
        let retention_rows: Vec<Vec<_>> = retained
            .iter()
            .enumerate()
            .map(|(g, group)| {
                group
                    .iter()
                    .enumerate()
                    .map(|(r, (t, p))| RetentionRowV1 {
                        encoded: encoded_decision_view_generic_v1(
                            &t.common,
                            FreshLineageGenerationV1::V4,
                        ),
                        expected_current_logits: &replay[g][r],
                        expected_current_value: current[g][r].value.to_bits(),
                        parent_logits: p,
                    })
                    .collect()
            })
            .collect();
        let retention_groups: Vec<_> = retention_rows
            .iter()
            .map(|rows| RetentionGroupV1 { rows })
            .collect();
        let result = state.train_step_retained_imitation_v4(
            &groups,
            &retention_groups,
            BETA,
            0.0001,
            c.workers,
        )?;
        let direct_ce = scores
            .iter()
            .zip(&targets)
            .map(|(s, &i)| cross_entropy(&s.logits, i))
            .sum::<f64>()
            / 32.0;
        ensure(
            (result.teacher_loss as f64 - direct_ce).abs() < 1e-4 + 1e-5 * direct_ce.abs(),
            "retained direct teacher loss differs",
        )?;
        if completed == 0 {
            ensure(
                result.retention_kl == 0.0 && result.weighted_retention_loss == 0.0,
                "identical parent must start at zero KL",
            )?;
            let mut legacy = original.clone();
            legacy
                .train_step_terminal_winner_imitation_v4(&groups, 0.0001, c.workers)
                .map_err(err)?;
            ensure(
                legacy.snapshot_v1().map_err(err)? == state.snapshot_v1().map_err(err)?,
                "initial zero-KL update differs from teacher",
            )?;
        }
        completed += 1;
        let snapshot = state.snapshot_v1().map_err(err)?;
        ensure(
            snapshot.adam_step == initial_adam + completed as u64,
            "retained Adam age differs",
        )?;
        policy.replace_training_parameters_v3(&snapshot.parameters)?;
        let saved = Saved {
            trajectory: c.trajectory.clone(),
            retention_dataset: c.retention_dataset.clone(),
            selected_rows: selected.clone(),
            checkpoint: Checkpoint {
                schema: RETAINED_SCHEMA.into(),
                loss_identity: RETAINED_LOSS.into(),
                source: c.source.clone(),
                dataset: c.dataset.clone(),
                target_permuted: false,
                backend: "cpu".into(),
                device_ordinal: 0,
                completed_updates: completed,
                initial_adam,
                adam_step: snapshot.adam_step,
                scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
                state_sha256: hex(&snapshot.state_sha256_v1().map_err(err)?),
                parameters: snapshot
                    .parameters
                    .iter()
                    .map(ParameterBitsV1::from_native)
                    .collect(),
                first_moments: snapshot
                    .first_moments
                    .iter()
                    .map(ParameterBitsV1::from_native)
                    .collect(),
                second_moments: snapshot
                    .second_moments
                    .iter()
                    .map(ParameterBitsV1::from_native)
                    .collect(),
            },
        };
        publish_json(
            &c.output_directory,
            &format!("checkpoint-{completed:03}.json"),
            &saved,
        )?;
        updates.push(json!({"update":completed,"teacher_loss":result.teacher_loss,"retention_kl":result.retention_kl,"weighted_retention_loss":result.weighted_retention_loss,"loss":result.loss,"state_sha256":saved.checkpoint.state_sha256}));
    }
    ensure(
        original_hash == hex(&original.state_sha256_v1().map_err(err)?),
        "retained original mutated",
    )?;
    let mut result = json!({"schema":RETAINED_SCHEMA,"complete":true,"initial_state":original_hash,"final_state":hex(&state.state_sha256_v1().map_err(err)?),"initial_adam":initial_adam,"final_adam":state.adam_step_v1(),"completed_updates":completed,"updates":updates,"selected_rows":selected,"parent_behavior_rows":exact,"teacher_positions":32,"retention_physical_groups":retained.len(),"retention_rows":retained.iter().map(Vec::len).sum::<usize>(),"beta":BETA,"evaluation_positions_read":0,"non_claim":"Two-update CPU integration fixture, not weight selection, representative retention or playing-strength evidence."});
    if let Some(pin) = &c.retention_dataset {
        result["retention_dataset"] = json!(pin);
        result["parent_score_replay_rows"] = json!(parent_score_replay_rows);
    }
    publish_json(&c.output_directory, "result.json", &result)?;
    publish_json(
        &c.output_directory,
        "timing.json",
        &json!({"seconds":started.elapsed().as_secs_f64(),"workers":c.workers}),
    )?;
    Ok(result)
}
