//! Read-only, final-only comparison of all three retained-training endpoints.
use super::*;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetContract {
    predecessors: BTreeMap<String, PinnedFileV1>,
    semantic_design: PinnedFileV1,
    semantic_checkpoint: PinnedFileV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    #[serde(default)]
    budget: Option<BudgetContract>,
    source: ExpandedModelSourceV1,
    teacher: PinnedFileV1,
    retention: PinnedFileV1,
    labels: PinnedFileV1,
    design: PinnedFileV1,
    checkpoints: Vec<PinnedFileV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixtures {
    contract: Contract,
    dataset: PinnedFileV1,
    training: bool,
    output_directory: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Natural {
    contract: Contract,
    trajectories: Vec<PinnedFileV1>,
    output_directory: PathBuf,
}

struct Model {
    label: String,
    checkpoint: Option<PinnedFileV1>,
    policy: FrozenPlayPolicyV1,
    state: NativePolicyValueTrainStateV1,
    state_hash: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeacherInferenceSource {
    schema: String,
    contract: Contract,
    playing_arm: String,
}

/// Restore the actual equal128 checkpoint through the existing strict reader.
/// Retain its CE identity and full Adam ancestry; never admit the wrong-label
/// semantic control as a playing source or reinterpret this as an RL update.
pub(super) fn load_teacher_inference(
    source: &ExpandedModelSourceV1,
) -> Result<(FrozenPlayPolicyV1, ExpandedInferenceIdentityV1), String> {
    let descriptor: TeacherInferenceSource =
        serde_json::from_slice(&read_pinned_bytes(&source.play_import)?).map_err(err)?;
    ensure(
        descriptor.schema == TEACHER_INFERENCE_SCHEMA
            && descriptor.contract.budget.is_some()
            && matches!(descriptor.playing_arm.as_str(), "unretained" | "retained"),
        "teacher inference requires a correct-label equal128 endpoint",
    )?;
    ensure(
        source.feature_transfer == descriptor.contract.source.feature_transfer,
        "teacher inference feature transfer differs",
    )?;
    let models = load(&descriptor.contract)?;
    let model = models
        .into_iter()
        .find(|m| m.label == descriptor.playing_arm)
        .ok_or("teacher inference arm missing")?;
    ensure(
        source.checkpoint.is_some() && same(&source.checkpoint, &model.checkpoint)?,
        "teacher inference checkpoint pin differs",
    )?;
    let identity = inference_identity_v1(source, &model.policy, &model.state)?;
    ensure(
        identity.state_sha256 == model.state_hash && identity.adam_step == 32528,
        "teacher inference restored state differs",
    )?;
    Ok((model.policy, identity))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherInferenceCheck {
    source: ExpandedModelSourceV1,
    output_directory: PathBuf,
}

/// Bounded engineering check through the public inference entry point. Only
/// the already consumed 32 teaching tensors are read, never a held-out panel.
pub(super) fn teacher_inference_check(c: TeacherInferenceCheck) -> Result<Value, String> {
    let (policy, identity) = load_expanded_inference_v1(&c.source)?;
    let descriptor: TeacherInferenceSource =
        serde_json::from_slice(&read_pinned_bytes(&c.source.play_import)?).map_err(err)?;
    let models = load(&descriptor.contract)?;
    let reference = models
        .iter()
        .find(|m| m.label == descriptor.playing_arm)
        .ok_or("teacher inference reference missing")?;
    ensure(
        identity.model == reference.policy.actual_model_identity_v1()
            && identity.state_sha256 == reference.state_hash
            && identity.adam_step == reference.state.adam_step_v1(),
        "teacher inference identity differs from evaluator",
    )?;
    ensure(
        initialize(&c.source).is_err(),
        "ordinary training unexpectedly accepts teacher inference source",
    )?;
    let data: Value =
        serde_json::from_slice(&read_pinned_bytes(&descriptor.contract.teacher)?).map_err(err)?;
    let rows = data["records"]
        .as_array()
        .ok_or("missing teacher records")?;
    ensure(
        rows.len() == 32,
        "teacher inference check requires32 training rows",
    )?;
    let mut records = Vec::new();
    for row in rows {
        let t: TensorBitsV1 = serde_json::from_value(row["data"]["tensor"].clone()).map_err(err)?;
        let tensor = NativeFlatDecisionTensorV4 { common: t.tensor() };
        let actual = policy.score_training_tensor_v4(&tensor)?;
        let expected = reference.policy.score_training_tensor_v4(&tensor)?;
        ensure(
            bits(&actual.logits) == bits(&expected.logits)
                && actual.value.to_bits() == expected.value.to_bits(),
            "teacher inference scores differ from evaluator",
        )?;
        records.push(json!({"id":row["id"],"logits_bits":bits(&actual.logits),
            "value_bits":actual.value.to_bits()}));
    }
    unchanged(&models)?;
    let result = json!({"schema":"terminal-teacher-inference-check/v1","complete":true,
        "source":c.source,"identity":identity,"playing_arm":descriptor.playing_arm,
        "positions":32,"updates":0,"new_games":0,"ordinary_training_rejected":true,
        "records":records,"non_claim":"Loader parity only; no whole-match or human-strength evidence."});
    fs::create_dir(&c.output_directory).map_err(err)?;
    publish_json(&c.output_directory, "result.json", &result)?;
    Ok(result)
}

fn same<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(serde_json::to_value(a).map_err(err)? == serde_json::to_value(b).map_err(err)?)
}

fn top(logits: &[f32]) -> usize {
    let mut best = 0;
    for i in 1..logits.len() {
        if logits[i] > logits[best] {
            best = i;
        }
    }
    best
}

fn probabilities(logits: &[f32]) -> Result<Vec<f64>, String> {
    ensure(
        !logits.is_empty() && logits.iter().all(|x| x.is_finite()),
        "invalid evaluation logits",
    )?;
    let mut sampler = WideCategoricalScratchV1::default();
    let masses = sampler.apportion(logits).map_err(err)?;
    ensure(
        masses.iter().sum::<u128>() == (1u128 << 64),
        "evaluation Hamilton mass differs",
    )?;
    Ok(masses
        .iter()
        .map(|&m| m as f64 / 18446744073709551616.0)
        .collect())
}

fn load(c: &Contract) -> Result<Vec<Model>, String> {
    ensure(
        c.checkpoints.len() == 3,
        "retained evaluation requires three final endpoints",
    )?;
    read_pinned_bytes(&c.design)?;
    let teacher: Value = serde_json::from_slice(&read_pinned_bytes(&c.teacher)?).map_err(err)?;
    let retention: Value =
        serde_json::from_slice(&read_pinned_bytes(&c.retention)?).map_err(err)?;
    let labels: Value = serde_json::from_slice(&read_pinned_bytes(&c.labels)?).map_err(err)?;
    let source = serde_json::to_value(&c.source).map_err(err)?;
    ensure(
        teacher["schema"] == "public-terminal-teacher-data/v1"
            && teacher["source"] == source
            && retention["schema"] == "terminal-parent-retention-data/v1"
            && retention["parent_source"] == source
            && labels["schema"] == "terminal-semantic-teacher-labels/v1"
            && labels["parent_source"] == source
            && labels["training"] == serde_json::to_value(&c.teacher).map_err(err)?,
        "retained evaluation input identities differ",
    )?;
    let groups = retention["groups"]
        .as_array()
        .ok_or("missing retained groups")?;
    ensure(
        groups.len() == 256,
        "retained evaluation training groups differ",
    )?;
    let selected = groups
        .iter()
        .map(|g| {
            serde_json::from_value::<Vec<usize>>(g["selection"]["archive_rows"].clone())
                .map_err(err)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (parent, parent_state) = initialize(&c.source)?;
    ensure(
        parent_state.adam_step_v1() == 32400
            && parent.feature_identity_v1().generation == FreshLineageGenerationV1::V4
            && teacher["model"]
                == serde_json::to_value(parent.actual_model_identity_v1()).map_err(err)?,
        "retained evaluation parent differs",
    )?;
    let template = parent_state.snapshot_v1().map_err(err)?;
    let mut models = vec![Model {
        label: "g115".into(),
        checkpoint: None,
        state_hash: hex(&parent_state.state_sha256_v1().map_err(err)?),
        policy: parent,
        state: parent_state,
    }];
    let mut arms = BTreeSet::new();
    if let Some(b) = &c.budget {
        ensure(
            b.predecessors
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                == BTreeSet::from(["unretained", "retained", "semantic"]),
            "equal budget predecessor set differs",
        )?;
        read_pinned_bytes(&b.semantic_design)?;
    }
    for pin in &c.checkpoints {
        let saved: retained_campaign::Saved =
            serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        let s = &saved.checkpoint;
        let label = if c.budget.is_some() && saved.arm == "semantic_retained" {
            "semantic"
        } else {
            saved.arm.as_str()
        };
        let lineage = if let Some(b) = &c.budget {
            let predecessor = b
                .predecessors
                .get(label)
                .ok_or("equal budget endpoint arm differs")?;
            let old: retained_campaign::Saved =
                serde_json::from_slice(&read_pinned_bytes(predecessor)?).map_err(err)?;
            let t = &old.checkpoint;
            read_pinned_bytes(&old.design)?;
            ensure(
                old.arm == label
                    && old.predecessor.is_none()
                    && old.trajectory.is_none()
                    && same(&old.labels, &c.labels)?
                    && same(&old.retention_dataset, &Some(c.retention.clone()))?
                    && old.selected_rows == selected
                    && same(&t.source, &c.source)?
                    && same(&t.dataset, &c.teacher)?
                    && t.schema == retained_campaign::RETAINED_SCHEMA
                    && t.loss_identity == retained_campaign::RETAINED_LOSS
                    && t.initial_adam == 32400
                    && t.completed_updates == 32
                    && t.adam_step == 32432
                    && t.target_permuted == (label == "semantic")
                    && t.backend == "cpu"
                    && t.device_ordinal == 0,
                "equal budget predecessor contract differs",
            )?;
            let predecessor_state = NativePolicyValueTrainSnapshotV1 {
                parameters: restore_parameters(&t.parameters, &template.parameters)?,
                first_moments: restore_parameters(&t.first_moments, &template.first_moments)?,
                second_moments: restore_parameters(&t.second_moments, &template.second_moments)?,
                adam_step: t.adam_step,
                scorer_bias_anchor_bits: t.scorer_bias_anchor_bits,
            };
            ensure(
                hex(&predecessor_state.state_sha256_v1().map_err(err)?) == t.state_sha256,
                "equal budget predecessor state hash differs",
            )?;
            same(&saved.predecessor, &Some(predecessor.clone()))?
                && s.completed_updates == 128
                && s.adam_step == 32528
                && if label == "semantic" {
                    same(pin, &b.semantic_checkpoint)?
                        && same(&saved.design, &b.semantic_design)?
                        && s.schema == "terminal-semantic-budget-diagnostic/v1"
                        && s.loss_identity == "terminal-semantic-ce-fixed128-budget/v1"
                } else {
                    same(&saved.design, &c.design)?
                        && s.schema == retained_campaign::EQUAL_SCHEMA
                        && s.loss_identity == retained_campaign::EQUAL_LOSS
                }
        } else {
            saved.predecessor.is_none()
                && same(&saved.design, &c.design)?
                && s.schema == retained_campaign::RETAINED_SCHEMA
                && s.loss_identity == retained_campaign::RETAINED_LOSS
                && s.completed_updates == 32
                && s.adam_step == 32432
        };
        ensure(
            matches!(label, "unretained" | "retained" | "semantic")
                && arms.insert(label.to_string())
                && lineage
                && same(&saved.labels, &c.labels)?
                && saved.trajectory.is_none()
                && same(&saved.retention_dataset, &Some(c.retention.clone()))?
                && saved.selected_rows == selected
                && same(&s.source, &c.source)?
                && same(&s.dataset, &c.teacher)?
                && s.initial_adam == 32400
                && s.target_permuted == (label == "semantic")
                && s.backend == "cpu"
                && s.device_ordinal == 0,
            "retained evaluation endpoint contract differs",
        )?;
        let snapshot = NativePolicyValueTrainSnapshotV1 {
            parameters: restore_parameters(&s.parameters, &template.parameters)?,
            first_moments: restore_parameters(&s.first_moments, &template.first_moments)?,
            second_moments: restore_parameters(&s.second_moments, &template.second_moments)?,
            adam_step: s.adam_step,
            scorer_bias_anchor_bits: s.scorer_bias_anchor_bits,
        };
        ensure(
            hex(&snapshot.state_sha256_v1().map_err(err)?) == s.state_sha256,
            "retained evaluation state hash differs",
        )?;
        let (mut policy, original) = initialize(&c.source)?;
        let state =
            NativePolicyValueTrainStateV1::from_snapshot_v1(original.model_v1().clone(), &snapshot)
                .map_err(err)?;
        policy.replace_training_parameters_v3(&snapshot.parameters)?;
        models.push(Model {
            label: label.to_string(),
            checkpoint: Some(pin.clone()),
            policy,
            state,
            state_hash: s.state_sha256.clone(),
        });
    }
    Ok(models)
}

fn unchanged(models: &[Model]) -> Result<(), String> {
    for m in models {
        ensure(
            hex(&m.state.state_sha256_v1().map_err(err)?) == m.state_hash,
            "retained evaluation mutated optimizer",
        )?;
    }
    Ok(())
}

pub(super) fn fixtures(c: Fixtures) -> Result<Value, String> {
    let models = load(&c.contract)?;
    let data: Value = serde_json::from_slice(&read_pinned_bytes(&c.dataset)?).map_err(err)?;
    ensure(
        data["source"] == serde_json::to_value(&c.contract.source).map_err(err)?,
        "retained fixture parent differs",
    )?;
    if c.training {
        ensure(
            same(&c.dataset, &c.contract.teacher)?,
            "training diagnostic dataset differs",
        )?;
    }
    let rows = data["records"]
        .as_array()
        .ok_or("missing retained evaluation fixtures")?;
    ensure(
        rows.len() == if c.training { 32 } else { 48 },
        "retained fixture count differs",
    )?;
    let labels: Value =
        serde_json::from_slice(&read_pinned_bytes(&c.contract.labels)?).map_err(err)?;
    let labels = labels["records"]
        .as_array()
        .ok_or("missing semantic targets")?;
    let mut ids = BTreeSet::new();
    let mut records = Vec::new();
    for row in rows {
        ensure(
            ids.insert(row["id"].as_str().ok_or("missing retained fixture id")?)
                && row["spec"]["split"] == if c.training { "train" } else { "evaluation" },
            "retained fixture split or duplicate differs",
        )?;
        let winners: Vec<usize> =
            serde_json::from_value(row["data"]["winning_indices"].clone()).map_err(err)?;
        let no_win = row["spec"]["no_win_control"]
            .as_bool()
            .ok_or("missing no-win flag")?;
        ensure(
            winners.len() == if no_win { 0 } else { 1 } && (!c.training || !no_win),
            "retained fixture winner count differs",
        )?;
        let outcomes = row["data"]["outcomes"]
            .as_array()
            .ok_or("missing fixture outcomes")?;
        for &winner in &winners {
            ensure(
                winner < outcomes.len()
                    && outcomes[winner]["classification"] == "win"
                    && outcomes[winner]["terminal"]["terminal_classification"] == "natural"
                    && outcomes[winner]["terminal"]["winner"]
                        == row["data"]["visible"]["acting_player"],
                "retained fixture lacks natural winning witness",
            )?;
        }
        let semantic = if c.training {
            let actor = row["spec"]["actor"]
                .as_u64()
                .ok_or("missing fixture actor")?;
            ensure(actor < 2, "invalid fixture actor")?;
            let actions = row["data"]["actions"]
                .as_array()
                .ok_or("missing fixture actions")?;
            let target = retained_campaign::semantic_target(actions, actor as u8, winners[0])?;
            let matches: Vec<_> = labels.iter().filter(|l| l["id"] == row["id"]).collect();
            ensure(
                matches.len() == 1
                    && matches[0]["winner"] == winners[0]
                    && matches[0]["control"] == target,
                "retained semantic target differs",
            )?;
            Some(target)
        } else {
            None
        };
        let t: TensorBitsV1 = serde_json::from_value(row["data"]["tensor"].clone()).map_err(err)?;
        let tensor = NativeFlatDecisionTensorV4 { common: t.tensor() };
        let mut scores = Vec::new();
        for m in &models {
            let s = m.policy.score_training_tensor_v4(&tensor)?;
            ensure(
                s.logits.len() == outcomes.len(),
                "retained fixture menu differs",
            )?;
            let p = probabilities(&s.logits)?;
            let best = top(&s.logits);
            let target = if c.training {
                Some(if m.label == "semantic" {
                    semantic.unwrap()
                } else {
                    winners[0]
                })
            } else {
                None
            };
            scores.push(json!({"label":m.label,"argmax":best,"argmax_wins":if no_win {None} else {Some(winners.contains(&best))},
                "training_target":target,"target_correct":target.map(|t|best==t),
                "target_cross_entropy":target.map(|t|cross_entropy(&s.logits,t)),
                "winning_probability":if no_win {None} else {Some(winners.iter().map(|&i|p[i]).sum::<f64>())},
                "probabilities":p,"logits_bits":bits(&s.logits),"value_bits":s.value.to_bits()}));
        }
        records.push(
            json!({"id":row["id"],"spec":row["spec"],"winning_indices":winners,"scores":scores}),
        );
    }
    unchanged(&models)?;
    let result = json!({"schema":"retained-terminal-fixture-evaluation/v1","complete":true,"updates":0,"training":c.training,
        "dataset":c.dataset,"positions":rows.len(),"models":models.iter().map(|m|json!({"label":m.label,"checkpoint":m.checkpoint,"state_sha256":m.state_hash})).collect::<Vec<_>>(),
        "records":records,"non_claim":"Correlated public tactical fixtures; no whole-match or human playing-strength claim."});
    fs::create_dir(&c.output_directory).map_err(err)?;
    publish_json(&c.output_directory, "result.json", &result)?;
    Ok(result)
}

pub(super) fn natural(c: Natural) -> Result<Value, String> {
    ensure(
        (1..=10).contains(&c.trajectories.len()),
        "retained natural archive bound differs",
    )?;
    let models = load(&c.contract)?;
    let parent = &models[0].policy;
    let mut archives = Vec::new();
    let mut ids = BTreeSet::new();
    for pin in &c.trajectories {
        let t: Trajectory = serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        ensure(
            t.schema == "mtg-kernel-public-stack-trajectory/v1"
                && !t.decisions.is_empty()
                && t.decisions.len() == t.auxiliary.len()
                && t.episode.learner_seat < 2
                && ids.insert(t.episode.id.clone()),
            "retained archive identity differs",
        )?;
        let opponent_is_parent = same(&t.episode.opponent, &Some(c.contract.source.clone()))?;
        let opponent = if opponent_is_parent {
            None
        } else {
            Some(
                initialize(
                    t.episode
                        .opponent
                        .as_ref()
                        .ok_or("missing archived opponent")?,
                )?
                .0,
            )
        };
        let mut rows = Vec::new();
        let mut replayed = 0;
        for (index, row) in t.decisions.iter().enumerate() {
            ensure(
                row.actor < 2 && (row.selected as usize) < row.logits.len(),
                "invalid retained archive action",
            )?;
            let tensor = NativeFlatDecisionTensorV4 {
                common: row.tensor.tensor(),
            };
            let p = parent.score_training_tensor_v4(&tensor)?;
            ensure(
                p.logits.len() == row.logits.len(),
                "retained parent menu differs",
            )?;
            if row.actor != t.episode.learner_seat {
                let other = opponent
                    .as_ref()
                    .map(|m| m.score_training_tensor_v4(&tensor))
                    .transpose()?;
                let replay = other.as_ref().unwrap_or(&p);
                ensure(
                    bits(&replay.logits) == row.logits && replay.value.to_bits() == row.value,
                    "retained archived opponent replay differs",
                )?;
                replayed += 1;
            }
            let pp = probabilities(&p.logits)?;
            let pt = top(&p.logits);
            let width = crate::native_flat_tensorizer_v2::NATIVE_FLAT_ACTION_FEATURE_DIM_V2;
            ensure(
                row.tensor.action_features.len() == p.logits.len() * width,
                "retained action feature shape differs",
            )?;
            let mut kinds = BTreeSet::new();
            for a in row.tensor.action_features.chunks_exact(width) {
                let one: Vec<_> = a[..27]
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| **v == 1f32.to_bits())
                    .map(|(i, _)| i)
                    .collect();
                ensure(
                    one.len() == 1
                        && a[..27]
                            .iter()
                            .all(|v| *v == 0f32.to_bits() || *v == 1f32.to_bits()),
                    "invalid retained action kind",
                )?;
                kinds.insert(one[0]);
            }
            let mut scores = Vec::new();
            for m in &models[1..] {
                let q = m.policy.score_training_tensor_v4(&tensor)?;
                ensure(
                    q.logits.len() == p.logits.len(),
                    "retained candidate menu differs",
                )?;
                let qp = probabilities(&q.logits)?;
                let qt = top(&q.logits);
                scores.push(json!({"label":m.label,"top":qt,"top_changed":pt!=qt,
                    "total_variation":pp.iter().zip(&qp).map(|(a,b)|(a-b).abs()).sum::<f64>()*0.5,
                    "probabilities":qp,"logits_bits":bits(&q.logits),"value_bits":q.value.to_bits(),
                    "absolute_value_change":(p.value as f64-q.value as f64).abs()}));
            }
            rows.push(json!({"archive_row":index,"step":row.step,"physical_decision_id":row.physical_decision_id,
                "substep_index":row.substep_index,"substep_count":row.substep_count,"actor":row.actor,
                "deck":t.episode.selected[row.actor as usize].label,"action_count":p.logits.len(),"action_kinds":kinds,
                "parent_top":pt,"parent_probabilities":pp,"parent_logits_bits":bits(&p.logits),"parent_value_bits":p.value.to_bits(),"scores":scores}));
        }
        ensure(replayed > 0, "retained archive has no opponent replay")?;
        archives.push(json!({"trajectory":pin,"episode_id":t.episode.id,"learner_seat":t.episode.learner_seat,
            "postboard":t.episode.postboard,"opponent_behavior_rows":replayed,"opponent_is_parent":opponent_is_parent,
            "opponent_source":t.episode.opponent,"rows":rows}));
    }
    unchanged(&models)?;
    let result = json!({"schema":"retained-terminal-natural-evaluation/v1","complete":true,"updates":0,"new_games":0,"outcomes_exported":false,
        "models":models.iter().map(|m|json!({"label":m.label,"checkpoint":m.checkpoint,"state_sha256":m.state_hash})).collect::<Vec<_>>(),"archives":archives,
        "non_claim":"Retention on archived visible decisions; agreement and value movement do not establish playing strength."});
    fs::create_dir(&c.output_directory).map_err(err)?;
    publish_json(&c.output_directory, "result.json", &result)?;
    Ok(result)
}
