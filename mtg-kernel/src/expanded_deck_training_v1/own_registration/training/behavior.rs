//! Offline CPU replay of pinned registration rows. No sessions, devices or draws.
use super::*;
use crate::fast_sampler::{
    WideCategoricalScratchV1, FAST_CATEGORICAL_MASS_TOTAL, WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
};
use crate::native_policy_value_net_v1::own_registration::RegistrationInputWeightsV1;
use crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1;

const REQUEST_SCHEMA: &str = "own-registration-saved-row-behavior-request/v1";
const ENDPOINT_LABELS: [&str; 5] = ["parent", "finalB", "finalR", "finalR-zeroW", "finalM"];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub panels: Vec<Panel>,
    pub output_directory: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    pub id: String,
    pub own_config: PinnedFileV1,
    pub own_checkpoint: PinnedFileV1,
    pub mean_config: PinnedFileV1,
    pub mean_checkpoint: PinnedFileV1,
    pub baseline_config: PinnedFileV1,
    pub baseline_checkpoint: PinnedFileV1,
    pub episodes: Vec<PinnedFileV1>,
}

struct Endpoint {
    label: &'static str,
    policy: OwnRegistrationPlayPolicyV1,
    identity: Value,
}

struct SelectedStep {
    step: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    actor: u8,
    selected: u32,
    tensor: NativeFlatDecisionTensorV4,
    source_auxiliary: RegistrationFeatureRowsV1,
    behavior_logits: Vec<u32>,
    behavior_value: u32,
    parent_output: Option<FrozenPlayDecisionScoresV1>,
}

struct SelectedGroup {
    learner_ordinal: usize,
    steps: Vec<SelectedStep>,
}

struct SelectedEpisode {
    source: PinnedFileV1,
    episode: ExpandedEpisodeV1,
    learner_groups: usize,
    groups: Vec<SelectedGroup>,
}

struct LoadedPanel {
    id: String,
    config_sha256: String,
    initial_optimizer_sha256: String,
    endpoints: Vec<Endpoint>,
    episodes: Vec<SelectedEpisode>,
}

fn fixed_panel(panel: &Panel, config: &Config) -> Result<(), String> {
    let parent_sha = match panel.id.as_str() {
        "r1-rep0" => "5c104b0d5bb532568549e9b2bdf6f787d32e51652b7154a148ffc94213c1a50b",
        "r2-rep0" => "d61079228505aac620302e3e5e608020f4ac4790343ea6a48979e4b10603da99",
        _ => return Err("saved-row panel must be r1-rep0 or r2-rep0".into()),
    };
    ensure(
        config
            .source
            .checkpoint
            .as_ref()
            .is_some_and(|p| p.sha256 == parent_sha)
            && panel.episodes.len() == 36,
        "saved-row panel parent or episode count differs",
    )
}

fn config_for(pin: &PinnedFileV1, arm: Arm, own: &Config) -> Result<Config, String> {
    let config: Config = read_pinned(pin)?;
    ensure(config.arm == arm, "saved-row endpoint arm differs")?;
    let mut normalized = config.clone();
    normalized.arm = Arm::Own;
    ensure(
        serde_json::to_value(&normalized).map_err(err)?
            == serde_json::to_value(own).map_err(err)?,
        "saved-row endpoint changes parent, schedule or recipe",
    )?;
    Ok(config)
}

fn restored_endpoint(
    config: &Config,
    checkpoint: &PinnedFileV1,
    base: &FrozenPlayPolicyV1,
    initial: &NativePolicyValueTrainSnapshotV1,
) -> Result<(Endpoint, NativePolicyValueTrainSnapshotV1), String> {
    let config_sha256 = sha(&serde_json::to_vec(config).map_err(err)?);
    let (legacy, projection, completed) = restore(config, &config_sha256, checkpoint, initial)?;
    ensure(
        completed == 9,
        "saved-row endpoint must complete nine updates",
    )?;
    let saved: Checkpoint = read_pinned(checkpoint)?;
    let label = match config.arm {
        Arm::Own => "finalR",
        Arm::Mean => "finalM",
        Arm::Baseline => "finalB",
    };
    let identity = json!({
        "kind":"registration-checkpoint", "checkpoint":checkpoint,
        "config_sha256":config_sha256,"optimizer_sha256":saved.optimizer_sha256,
        "arm":config.arm,"mode":projection.mode,"projection_enabled":projection.projection_enabled,
        "base_adam_step":legacy.adam_step,"projection_adam_step":projection.adam_step,
        "completed_updates":completed,"training_math":config.training_math,
    });
    let policy = policy(base.fork_for_collection_v3()?, &legacy, &projection)?;
    Ok((
        Endpoint {
            label,
            policy,
            identity,
        },
        legacy,
    ))
}

fn select_episode(
    source: &PinnedFileV1,
    expected: &ExpandedEpisodeV1,
    panel: &LoadedPanel,
) -> Result<SelectedEpisode, String> {
    let trajectory: Trajectory = read_pinned(source)?;
    ensure(
        trajectory.config_sha256 == panel.config_sha256
            && trajectory.optimizer_state_sha256 == panel.initial_optimizer_sha256
            && trajectory.mode == RegistrationModeV1::Own
            && trajectory.projection_enabled
            && trajectory.learner_sampler.is_none()
            && serde_json::to_value(&trajectory.episode).map_err(err)?
                == serde_json::to_value(expected).map_err(err)?,
        "saved-row trajectory is not the specified initial Own episode",
    )?;
    ensure(
        trajectory
            .opponent
            .as_ref()
            .and_then(|p| p.checkpoint_sha256.as_ref())
            == expected
                .opponent
                .as_ref()
                .and_then(|p| p.checkpoint.as_ref())
                .map(|p| &p.sha256),
        "saved-row fixed opponent binding differs",
    )?;
    let groups = learner_groups(&trajectory)?;
    let count = groups.len();
    ensure(
        count > 0,
        "saved-row episode has no learner physical groups",
    )?;
    let chosen: BTreeSet<_> = [0, count / 2, count - 1].into_iter().collect();
    let mut selected = Vec::new();
    for (ordinal, group) in groups.into_iter().enumerate() {
        if !chosen.contains(&ordinal) {
            continue;
        }
        ensure(
            group.len() <= 128,
            "saved-row physical group exceeds inherited chunk bound",
        )?;
        let steps = group
            .into_iter()
            .map(|(row, tensor, auxiliary)| SelectedStep {
                step: row.step,
                physical_decision_id: row.physical_decision_id,
                substep_index: row.substep_index,
                substep_count: row.substep_count,
                actor: row.actor,
                selected: row.selected,
                tensor,
                source_auxiliary: auxiliary.clone(),
                behavior_logits: row.logits.clone(),
                behavior_value: row.value,
                parent_output: None,
            })
            .collect();
        selected.push(SelectedGroup {
            learner_ordinal: ordinal,
            steps,
        });
    }
    Ok(SelectedEpisode {
        source: source.clone(),
        episode: trajectory.episode,
        learner_groups: count,
        groups: selected,
    })
}

fn load_panel(panel: &Panel) -> Result<LoadedPanel, String> {
    let own: Config = read_pinned(&panel.own_config)?;
    fixed_panel(panel, &own)?;
    validate_config(&own)?;
    let parent_name = panel
        .id
        .split('-')
        .next()
        .ok_or("missing saved-row parent")?;
    let episode_prefix =
        format!("own-registration-strength-pilot-v1/2026-10-17/train/{parent_name}/rep0/");
    ensure(
        own.arm == Arm::Own
            && own.updates.len() == 9
            && own.updates.iter().all(|batch| batch.len() == 36)
            && own.max_chunk_substeps == 128
            && own.updates.iter().flatten().all(|e| {
                e.opponent.is_some() && e.reset.is_none() && e.id.starts_with(&episode_prefix)
            }),
        "saved-row diagnostic requires the fixed ordinary nine-by36 recipe",
    )?;
    let mean = config_for(&panel.mean_config, Arm::Mean, &own)?;
    let baseline = config_for(&panel.baseline_config, Arm::Baseline, &own)?;
    let (base, initial) = initialize(&own.source)?;
    ensure(
        base.training_math_v1() == TrainingMathV1::PlatformLibm,
        "saved-row parent math differs",
    )?;
    let initial = initial.snapshot_v1().map_err(err)?;
    let initial_state_sha256 = hex(&initial.state_sha256_v1().map_err(err)?);
    let parent = Endpoint {
        label: "parent",
        identity: json!({"kind":"ordinary-parent","source":own.source,
            "model":base.actual_model_identity_v1(),"state_sha256":initial_state_sha256,
            "base_adam_step":initial.adam_step,"training_math":own.training_math}),
        policy: policy(
            base.fork_for_collection_v3()?,
            &initial,
            &ProjectionSnapshot::baseline().map_err(err)?,
        )?,
    };
    let (final_own, restored_own) =
        restored_endpoint(&own, &panel.own_checkpoint, &base, &initial)?;
    let mut zero_policy = final_own.policy.fork_for_collection()?;
    zero_policy.install(&restored_own.parameters, RegistrationInputWeightsV1::zero())?;
    let zero = Endpoint {
        label: "finalR-zeroW",
        policy: zero_policy,
        identity: json!({"kind":"read-only-ablation","source":final_own.identity,
            "ablation":"positive-zero W only; inherited finalR parameters unchanged",
            "inherited_state_sha256":hex(&restored_own.state_sha256_v1().map_err(err)?),
            "mode":"own","projection_enabled":true}),
    };
    drop(restored_own);
    let (final_mean, _) = restored_endpoint(&mean, &panel.mean_checkpoint, &base, &initial)?;
    let (final_baseline, _) =
        restored_endpoint(&baseline, &panel.baseline_checkpoint, &base, &initial)?;
    let initial_projection = Arm::Own.initial()?;
    let mut loaded = LoadedPanel {
        id: panel.id.clone(),
        config_sha256: sha(&serde_json::to_vec(&own).map_err(err)?),
        initial_optimizer_sha256: sha(&device_training::snapshot::encode(
            &initial,
            &initial_projection,
        )
        .map_err(err)?),
        endpoints: vec![parent, final_baseline, final_own, zero, final_mean],
        episodes: Vec::new(),
    };
    for (source, episode) in panel.episodes.iter().zip(&own.updates[0]) {
        // Only selected owned tensors survive this call; the complete trajectory
        // and all unselected reconstructed tensors are dropped before the next file.
        let selected = select_episode(source, episode, &loaded)?;
        loaded.episodes.push(selected);
    }
    Ok(loaded)
}

fn parent_parity(panel: &mut LoadedPanel) -> Result<usize, String> {
    let parent = &panel.endpoints[0];
    let mut count = 0;
    for saved in &mut panel.episodes {
        let configs = saved.episode.configurations()?;
        for group in &mut saved.groups {
            for row in &mut group.steps {
                let expected = expectation(&saved.episode, &configs, row.actor)?;
                let context = KnownOwnMainboardV1::new(
                    &saved.episode.id,
                    actor_seat(row.actor)?,
                    &configs[usize::from(row.actor)],
                )?;
                let rows = context.rows(RegistrationModeV1::Own)?;
                ensure(
                    rows == row.source_auxiliary,
                    "saved-row context differs from selected configuration",
                )?;
                let output = parent.policy.replay(&row.tensor, &rows, expected)?;
                ensure(
                    bits(&output.logits) == row.behavior_logits
                        && output.value.to_bits() == row.behavior_value,
                    &format!(
                        "saved-row parent parity differs: panel={} episode={} step={}",
                        panel.id, saved.episode.id, row.step
                    ),
                )?;
                row.parent_output = Some(output);
                count += 1;
            }
        }
    }
    Ok(count)
}

fn output_scores(
    output: &FrozenPlayDecisionScoresV1,
    sampler: &mut WideCategoricalScratchV1,
) -> Result<Value, String> {
    // Do not center/re-normalize f32 logits before the exact Q8 sampler.
    let masses = sampler.apportion(&output.logits).map_err(err)?;
    ensure(
        masses.iter().copied().sum::<u128>() == FAST_CATEGORICAL_MASS_TOTAL,
        "saved-row mass sum differs",
    )?;
    Ok(
        json!({"logits_bits":bits(&output.logits),"value_bits":output.value.to_bits(),
        "mass_numerators":masses.iter().map(u128::to_string).collect::<Vec<_>>(),
        "mass_denominator":FAST_CATEGORICAL_MASS_TOTAL.to_string()}),
    )
}

fn comparisons(panel: &LoadedPanel) -> Result<Value, String> {
    let mut output_rows = Vec::new();
    let mut sampler = WideCategoricalScratchV1::default();
    let mut forward_calls = 0;
    for (episode_ordinal, saved) in panel.episodes.iter().enumerate() {
        let configs = saved.episode.configurations()?;
        for group in &saved.groups {
            for row in &group.steps {
                let expected = expectation(&saved.episode, &configs, row.actor)?;
                let context = KnownOwnMainboardV1::new(
                    &saved.episode.id,
                    actor_seat(row.actor)?,
                    &configs[usize::from(row.actor)],
                )?;
                let mut outputs = serde_json::Map::new();
                let parent = row
                    .parent_output
                    .as_ref()
                    .ok_or("saved-row comparison before parent parity")?;
                outputs.insert("parent".into(), output_scores(parent, &mut sampler)?);
                for endpoint in &panel.endpoints[1..] {
                    let rows = context.rows(endpoint.policy.mode())?;
                    let output = endpoint.policy.replay(&row.tensor, &rows, expected)?;
                    ensure(
                        output.logits.len() == row.behavior_logits.len(),
                        "saved-row endpoint changed menu width",
                    )?;
                    outputs.insert(endpoint.label.into(), output_scores(&output, &mut sampler)?);
                    forward_calls += 1;
                }
                output_rows.push(json!({
                    "source_episode_ordinal":episode_ordinal,"source_episode":saved.source,
                    "external_episode_id":saved.episode.id,"seed":saved.episode.seed,
                    "learner_seat":saved.episode.learner_seat,"actor":row.actor,
                    "selected_mainboard_sha256":hex(&configs[usize::from(row.actor)].mainboard_sha256_v1()),
                    "source_learner_group_ordinal":group.learner_ordinal,
                    "source_learner_group_count":saved.learner_groups,
                    "step":row.step,"physical_decision_id":row.physical_decision_id,
                    "substep_index":row.substep_index,"substep_count":row.substep_count,
                    "action_count":row.behavior_logits.len(),"singleton":row.behavior_logits.len()==1,
                    "behavior_selected":row.selected,"behavior_logits_bits":row.behavior_logits,
                    "behavior_value_bits":row.behavior_value,
                    "tensor_sha256":sha(&serde_json::to_vec(&TensorBitsV1::from_tensor(&row.tensor.common)).map_err(err)?),
                    "endpoints":outputs,
                }));
            }
        }
    }
    Ok(
        json!({"schema":"own-registration-saved-row-panel/v1","id":panel.id,
        "own_config_sha256":panel.config_sha256,"initial_optimizer_sha256":panel.initial_optimizer_sha256,
        "endpoint_identities":panel.endpoints.iter().map(|e| json!({"label":e.label,"identity":e.identity})).collect::<Vec<_>>(),
        "comparison_forward_calls":forward_calls,"rows":output_rows}),
    )
}

/// Reads pinned saved tensors and CPU snapshots only. Compiling with the CUDA
/// feature does not construct a CUDA device or call the training update path.
pub fn run(request: Request) -> Result<Value, String> {
    ensure(
        request.schema == REQUEST_SCHEMA,
        "saved-row request schema differs",
    )?;
    ensure(
        request.output_directory.is_absolute() && !request.output_directory.exists(),
        "saved-row output must be fresh and absolute",
    )?;
    ensure(
        request.panels.len() == 2
            && request.panels[0].id == "r1-rep0"
            && request.panels[1].id == "r2-rep0",
        "saved-row request requires ordered r1-rep0 and r2-rep0 panels",
    )?;
    let request_sha256 = sha(&serde_json::to_vec(&request).map_err(err)?);
    let mut loaded = request
        .panels
        .iter()
        .map(load_panel)
        .collect::<Result<Vec<_>, _>>()?;
    let mut parity_rows = 0;
    for panel in &mut loaded {
        parity_rows += parent_parity(panel)?;
    }
    // Every selected parent row in BOTH panels has passed before any final
    // endpoint is forwarded, or any diagnostic output is published.
    let reports = loaded
        .iter()
        .map(comparisons)
        .collect::<Result<Vec<_>, _>>()?;
    fs::create_dir(&request.output_directory).map_err(err)?;
    let request_pin = publish_json(&request.output_directory, "request.json", &request)?;
    let mut panel_pins = Vec::new();
    for (panel, report) in loaded.iter().zip(reports) {
        panel_pins.push(publish_json(
            &request.output_directory,
            &format!("{}.json", panel.id),
            &report,
        )?);
    }
    let completion = json!({"schema":"own-registration-saved-row-completion/v1",
        "request_sha256":request_sha256,"request":request_pin,"panels":panel_pins,
        "parent_parity_rows":parity_rows,"parent_forward_calls":parity_rows,
        "comparison_forward_calls":parity_rows*4,"endpoint_labels":ENDPOINT_LABELS,
        "sampler":WIDE_CATEGORICAL_SAMPLER_VERSION_V1,"sampler_draws":0,
        "mass_denominator":FAST_CATEGORICAL_MASS_TOTAL.to_string(),
        "selection":"first, floor(n/2), last learner physical group; deduplicated; all substeps",
        "interpretation":"conditional saved prefixes; not newly sampled trajectories or terminal-strength evidence",
        "games":0,"updates":0,"gpu_devices":0});
    publish_json(&request.output_directory, "completion.json", &completion)?;
    Ok(completion)
}
