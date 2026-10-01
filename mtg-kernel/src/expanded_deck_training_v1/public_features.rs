//! Bounded synchronous public-feature learning, with distinct trajectory and
//! checkpoint identities. No legacy trajectory is relabeled as a successor.
use super::*;
use crate::experimental_burn_net8_packed_v1::public_training::{
    self, ProjectionSnapshot, PublicDeviceTrainState, PublicTrainingGroup, PublicTrainingStep,
};
use crate::native_flat_tensorizer_v4::encoded_decision_view_v4;
use crate::native_policy_value_net_v1::public_inputs_v1::PublicInputWeightsV1;
use crate::paired_bo1_harness_v1::{PairedBo1PolicyInputV1, PairedBo1PolicyV1};
use crate::public_cost_features_v1::PublicFeatureRowsV1;
use crate::sideboard_play_policy_v1::public_inputs::PublicInputPlayPolicyV1;

pub(crate) mod opponent_kind;
pub mod replay_audit;
pub(crate) mod search_opponent;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMode {
    #[default]
    All,
    StateOnly,
}

impl ProjectionMode {
    fn is_all(&self) -> bool {
        *self == Self::All
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub source: ExpandedModelSourceV1,
    pub updates: Vec<Vec<ExpandedEpisodeV1>>,
    pub inputs_enabled: bool,
    pub learning_rate: f32,
    pub value_coefficient: f32,
    pub gamma: f32,
    pub lambda: f32,
    pub gpu_ordinal: usize,
    pub max_chunk_substeps: usize,
    /// Omitted default preserves every prior serialized configuration/hash.
    #[serde(default, skip_serializing_if = "ProjectionMode::is_all")]
    pub projection_mode: ProjectionMode,
    /// A learning-only opt-in. Zero preserves all prior config bytes/hashes.
    #[serde(default, skip_serializing_if = "entropy_is_zero")]
    pub entropy_coefficient: f32,
}

fn entropy_is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn validate_entropy(config: &Config) -> Result<(), String> {
    ensure(
        config.entropy_coefficient.is_finite() && (0.0..=1.0).contains(&config.entropy_coefficient),
        "public entropy coefficient must be finite in [0,1]",
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub config: Config,
    pub output_directory: PathBuf,
    pub resume: Option<PinnedFileV1>,
    pub stop_after: Option<usize>,
    /// Execution placement only; excluded from the scientific config hash.
    #[serde(default = "default_collector_workers")]
    pub collector_workers: usize,
    /// Explicit execution placement; preserves the frozen scientific config.
    /// Omission retains its original device assignment.
    #[serde(default)]
    pub execution_gpu_ordinal: Option<usize>,
    /// Execution-only diagnostic (FABLE-REVIEW-20260927 change 2): audit every
    /// Nth search-opponent decision of each game against perturbations of the
    /// hidden state. Excluded from the config hash; trajectories are unchanged;
    /// counts go to search-opponent-audit.json. None audits nothing.
    #[serde(default)]
    pub search_boundary_audit_every: Option<u32>,
}

fn default_collector_workers() -> usize {
    1
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trajectory {
    schema: String,
    config_sha256: String,
    optimizer_state_sha256: String,
    inputs_enabled: bool,
    episode: ExpandedEpisodeV1,
    /// The loaded net's inference identity for ordinary and D3 opponents.
    /// None for an opt-in opponent kind, whose effective identity lives in
    /// `opponent_record`. Some serializes exactly as the former bare field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opponent: Option<ExpandedInferenceIdentityV1>,
    configuration_sha256: [String; 2],
    decisions: Vec<DecisionRecordV1>,
    /// Absent on the existing public collector, which uses legacy draws.
    /// Replay must carry the recorded identity through both opponent hooks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    learner_sampler: Option<String>,
    auxiliary: Vec<Option<PublicFeatureRowsV1>>,
    terminal: RlSessionTerminalV1,
    /// Present only when the D3 wrapper played the opponent seat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search: Option<search_opponent::SearchTrajectoryV1>,
    /// Present only for an opt-in opponent kind (opponent kinds interface v1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opponent_record: Option<opponent_kind::OpponentRecordV1>,
}

impl Trajectory {
    fn validate_records(&self) -> Result<(), String> {
        let sampler = self.learner_sampler.as_deref();
        match (&self.search, &self.opponent_record) {
            (None, None) => validate_episode_records_with_learner_sampler_v1(
                &self.episode,
                &self.configuration_sha256,
                &self.decisions,
                &self.terminal,
                sampler,
            ),
            (Some(record), None) => record.validate(
                &self.episode,
                &self.configuration_sha256,
                &self.decisions,
                &self.terminal,
                sampler,
            ),
            (None, Some(record)) => record.validate(
                &self.episode,
                &self.configuration_sha256,
                &self.decisions,
                &self.terminal,
                sampler,
            ),
            (Some(_), Some(_)) => Err("two opponent records".into()),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: String,
    config_sha256: String,
    next_update: usize,
    optimizer_file: String,
    optimizer_sha256: String,
    trajectory_sha256: Vec<String>,
}

fn weights(public: &ProjectionSnapshot) -> Result<PublicInputWeightsV1, String> {
    PublicInputWeightsV1::new(floats(&public.object), floats(&public.state)).map_err(err)
}

fn validate_projection_mode(config: &Config, public: &ProjectionSnapshot) -> Result<(), String> {
    if !config.inputs_enabled || config.projection_mode == ProjectionMode::StateOnly {
        ensure(
            public
                .object
                .iter()
                .chain(&public.object_first)
                .chain(&public.object_second)
                .all(|v| f32::from_bits(*v) == 0.0),
            "disabled public cost projection acquired weights or moments",
        )?;
    }
    if !config.inputs_enabled {
        ensure(
            public
                .state
                .iter()
                .chain(&public.state_first)
                .chain(&public.state_second)
                .all(|v| f32::from_bits(*v) == 0.0),
            "disabled public state projection acquired weights or moments",
        )?;
    }
    Ok(())
}

/// CPU inference load only. Reading a CUDA-produced checkpoint creates no GPU
/// device and cannot advance either optimizer.
pub(crate) fn load_for_evaluation(
    config_pin: &PinnedFileV1,
    checkpoint_pin: &PinnedFileV1,
) -> Result<(PublicInputPlayPolicyV1, Value), String> {
    let config: Config = serde_json::from_slice(&read_pinned_bytes(config_pin)?).map_err(err)?;
    validate_entropy(&config)?;
    let checkpoint: Checkpoint =
        serde_json::from_slice(&read_pinned_bytes(checkpoint_pin)?).map_err(err)?;
    let config_hash = sha(&serde_json::to_vec(&config).map_err(err)?);
    ensure(
        checkpoint.schema == "mtg-kernel-public-input-checkpoint/v1"
            && checkpoint.config_sha256 == config_hash
            && checkpoint.optimizer_file == "optimizer.json"
            && checkpoint.next_update > 0
            && checkpoint.next_update <= config.updates.len(),
        "public evaluation checkpoint/config identity differs",
    )?;
    let optimizer_path = checkpoint_pin
        .path
        .parent()
        .ok_or("checkpoint has no parent")?
        .join("optimizer.json");
    let saved = read_pinned_bytes(&PinnedFileV1 {
        path: optimizer_path,
        sha256: checkpoint.optimizer_sha256.clone(),
    })?;
    let (legacy, public) = public_training::snapshot::decode(&saved).map_err(err)?;
    validate_projection_mode(&config, &public)?;
    let (mut base, initial) = initialize(&config.source)?;
    ensure(
        legacy.adam_step == initial.adam_step_v1() + checkpoint.next_update as u64
            && public.adam_step == checkpoint.next_update as u64,
        "public evaluation ages differ",
    )?;
    base.replace_training_parameters_v3(&legacy.parameters)?;
    let base_model = base.actual_model_identity_v1();
    let weights_sha256 = sha(&serde_json::to_vec(&(
        crate::native_policy_value_net_v1::public_inputs_v1::ARCHITECTURE,
        &base_model.weights_sha256,
        &public.object,
        &public.state,
        config.inputs_enabled,
    ))
    .map_err(err)?);
    let mut identity = json!({"schema":"public-input-evaluation-model/v1",
        "architecture":crate::native_policy_value_net_v1::public_inputs_v1::ARCHITECTURE,
        "config_pin":config_pin,"checkpoint_pin":checkpoint_pin,"config_sha256":config_hash,
        "optimizer_sha256":checkpoint.optimizer_sha256,"weights_sha256":weights_sha256,
        "actual_base_model":base_model,"inputs_enabled":config.inputs_enabled,
        "legacy_adam_step":legacy.adam_step,"public_adam_step":public.adam_step});
    if config.projection_mode == ProjectionMode::StateOnly {
        identity["projection_mode"] = json!("state_only");
    }
    Ok((
        PublicInputPlayPolicyV1::new(base, weights(&public)?)?
            .with_inputs_enabled(config.inputs_enabled),
        identity,
    ))
}

fn collect(
    policy: &mut PublicInputPlayPolicyV1,
    episode: &ExpandedEpisodeV1,
    config_hash: &str,
    state_hash: &str,
    enabled: bool,
    audit_every: Option<u32>,
) -> Result<Trajectory, String> {
    let seat = match &episode.opponent_kind {
        Some(kind) => opponent_kind::load_seat(kind, episode)?,
        None => {
            let (net, identity) = load_expanded_inference_v1(
                episode
                    .opponent
                    .as_ref()
                    .ok_or("explicit opponent required")?,
            )?;
            ensure(
                net.feature_identity_v1().generation == FreshLineageGenerationV1::V4,
                "public collection requires V4 opponent",
            )?;
            let search = episode
                .opponent_search
                .as_ref()
                .map(|pin| search_opponent::SearchOpponentV1::load(pin, episode, &net, &identity))
                .transpose()?;
            opponent_kind::OpponentSeatV1::Net {
                net,
                identity,
                search,
            }
        }
    };
    collect_with_opponent(
        policy,
        episode,
        config_hash,
        state_hash,
        enabled,
        audit_every,
        seat,
    )
}

/// One game with an already loaded opponent seat: an ordinary V4 net, the
/// D3 wrapper around it, or an opt-in opponent kind. Split from `collect` so
/// fixture policies can drive it.
fn collect_with_opponent(
    policy: &mut PublicInputPlayPolicyV1,
    episode: &ExpandedEpisodeV1,
    config_hash: &str,
    state_hash: &str,
    enabled: bool,
    audit_every: Option<u32>,
    mut opponent: opponent_kind::OpponentSeatV1,
) -> Result<Trajectory, String> {
    use opponent_kind::OpponentSeatV1 as Seat;
    let configs = episode.configurations_for_public_collector_v1()?;
    ensure(
        match &opponent {
            Seat::Net { search, .. } => {
                episode.opponent_kind.is_none()
                    && search.is_some() == episode.opponent_search.is_some()
            }
            Seat::PublicCheckpoint { .. } => matches!(
                episode.opponent_kind,
                Some(ExpandedOpponentKindV1::PublicCheckpoint { .. })
            ),
            Seat::Legacy { .. } => matches!(
                episode.opponent_kind,
                Some(ExpandedOpponentKindV1::Legacy { .. })
            ),
        },
        "opponent dispatch differs from its episode",
    )?;
    let mut session=FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        1,episode.seed,episode.max_physical_decisions,episode.max_policy_steps,episode.selected.each_ref().map(|d|d.label.clone()),configs.each_ref().map(|c|c.mainboard().to_vec()),PlayerId(episode.starting_player)).map_err(err)?;
    let seeds = paired_policy_seeds_v1(episode.seed);
    policy.reset_for_game_v1(seeds).map_err(err)?;
    match &mut opponent {
        Seat::Net { net, search, .. } => {
            net.reset_sampling_v1(seeds);
            if let Some(search) = search.as_mut() {
                search.reset_for_game(seeds, &episode.id)?;
            }
        }
        Seat::PublicCheckpoint { policy, .. } => policy.reset_for_game_v1(seeds).map_err(err)?,
        Seat::Legacy { policy, .. } => policy.reset_for_game_v1(seeds).map_err(err)?,
    }
    let mut decisions = Vec::new();
    let mut auxiliary = Vec::new();
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                let hashes = configs.each_ref().map(|c| hex(&c.mainboard_sha256_v1()));
                let schema = opponent.outer_schema();
                let (identity, search, opponent_record) = match opponent {
                    Seat::Net {
                        identity, search, ..
                    } => (
                        Some(identity),
                        search.as_ref().map(|s| s.finish()).transpose()?,
                        None,
                    ),
                    Seat::PublicCheckpoint { identity, rows, .. } => (
                        None,
                        None,
                        Some(opponent_kind::OpponentRecordV1::new(
                            "public_checkpoint",
                            1 - episode.learner_seat,
                            identity,
                            rows,
                        )),
                    ),
                    Seat::Legacy { identity, rows, .. } => (
                        None,
                        None,
                        Some(opponent_kind::OpponentRecordV1::new(
                            "legacy",
                            1 - episode.learner_seat,
                            identity,
                            rows,
                        )),
                    ),
                };
                let trajectory = Trajectory {
                    schema: schema.into(),
                    config_sha256: config_hash.into(),
                    optimizer_state_sha256: state_hash.into(),
                    inputs_enabled: enabled,
                    episode: episode.clone(),
                    opponent: identity,
                    configuration_sha256: hashes,
                    decisions,
                    learner_sampler: None,
                    auxiliary,
                    terminal,
                    search,
                    opponent_record,
                };
                trajectory.validate_records()?;
                return Ok(trajectory);
            }
            FastActorResponseV1::Decision(d) => {
                ensure(
                    decisions.len() < episode.max_policy_steps as usize,
                    "public collection step limit",
                )?;
                let learner = seat(d.acting_player) == episode.learner_seat;
                let mut sampler_identity = None;
                let (selected, scores, tensor) = if learner {
                    let input = PairedBo1PolicyInputV1::new(&session, d);
                    let (selected, scores) = policy.select_with_scores(&input).map_err(err)?;
                    let (tensor, rows) = policy.captured()?;
                    auxiliary.push(Some(rows.clone()));
                    (selected, scores, TensorBitsV1::from_tensor(&tensor.common))
                } else {
                    match &mut opponent {
                        Seat::Net {
                            net,
                            search: Some(search),
                            ..
                        } => {
                            let (selected, scores, tensor) = search.select(net, &session, d)?;
                            if audit_every
                                .is_some_and(|n| search.decisions() % u64::from(n.max(1)) == 0)
                            {
                                search_opponent::audit_live_root(
                                    search, net, &session, d, selected,
                                )?;
                            }
                            auxiliary.push(None);
                            sampler_identity =
                                Some(search_opponent::SEARCH_SAMPLER_IDENTITY.to_owned());
                            (selected, scores, TensorBitsV1::from_tensor(&tensor.common))
                        }
                        Seat::Net {
                            net, search: None, ..
                        } => {
                            let (selected, scores, tensor) =
                                net.select_with_training_tensor_v4(&session)?;
                            auxiliary.push(None);
                            (selected, scores, TensorBitsV1::from_tensor(&tensor.common))
                        }
                        Seat::PublicCheckpoint { policy, rows, .. } => {
                            let input = PairedBo1PolicyInputV1::new(&session, d);
                            let (selected, scores) =
                                policy.select_with_scores(&input).map_err(err)?;
                            let (tensor, public_rows) = policy.captured()?;
                            auxiliary.push(Some(public_rows.clone()));
                            Seat::push_public_row(
                                rows,
                                &session,
                                d,
                                selected,
                                scores.logits.len(),
                            )?;
                            (selected, scores, TensorBitsV1::from_tensor(&tensor.common))
                        }
                        Seat::Legacy {
                            policy,
                            forced,
                            spell_adapter,
                            rows,
                            ..
                        } => {
                            let (selected, scores, tensor, identity) = Seat::legacy_decide(
                                policy,
                                *forced,
                                *spell_adapter,
                                rows,
                                &session,
                                d,
                            )?;
                            auxiliary.push(None);
                            sampler_identity = identity;
                            (selected, scores, tensor)
                        }
                    }
                };
                decisions.push(DecisionRecordV1 {
                    step: d.step,
                    physical_decision_id: d.physical_decision_id,
                    substep_index: d.substep_index,
                    substep_count: d.substep_count,
                    actor: seat(d.acting_player),
                    selected,
                    logits: bits(&scores.logits),
                    value: scores.value.to_bits(),
                    tensor,
                    sampler_identity: sampler_identity.or_else(|| {
                        decision_sampler_identity_v1(scores.logits.len()).map(str::to_owned)
                    }),
                });
                session.step(d.episode_id, d.step, selected).map_err(err)?;
            }
        }
    }
}

fn publish_bytes(directory: &Path, name: &str, bytes: &[u8]) -> Result<String, String> {
    ensure(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "public output size exceeds bound",
    )?;
    let parent = capture_existing_publication_parent_v1(directory).map_err(err)?;
    publish_new_file_v1(
        &parent,
        format!(".{name}.stage"),
        name,
        bytes,
        DurableFileExpectationV1::from_bytes(bytes).map_err(err)?,
    )
    .map_err(err)?;
    Ok(sha(bytes))
}

/// The update's input from one trajectory: the learner's physical-decision
/// groups in order, each row with its tensor and public row. The learner
/// filter comes first, so no opponent row, record or auxiliary row enters.
pub(super) fn learner_groups(
    trajectory: &Trajectory,
) -> Result<
    Vec<
        Vec<(
            &DecisionRecordV1,
            NativeFlatDecisionTensorV4,
            &PublicFeatureRowsV1,
        )>,
    >,
    String,
> {
    let mut groups = Vec::new();
    let mut index = 0;
    while index < trajectory.decisions.len() {
        let first = &trajectory.decisions[index];
        let end = index + first.substep_count as usize;
        if first.actor == trajectory.episode.learner_seat {
            let mut group = Vec::new();
            for i in index..end {
                let row = &trajectory.decisions[i];
                let auxiliary = trajectory.auxiliary[i]
                    .as_ref()
                    .ok_or("missing learner public row")?;
                let tensor = NativeFlatDecisionTensorV4 {
                    common: row.tensor.tensor(),
                };
                group.push((row, tensor, auxiliary));
            }
            groups.push(group);
        }
        index = end;
    }
    Ok(groups)
}

/// All collectors use the current batch's parameters. Results are ordered by
/// the original schedule before publication and the single learning update.
fn collect_parallel(
    policy: &PublicInputPlayPolicyV1,
    episodes: &[ExpandedEpisodeV1],
    config_hash: &str,
    state_hash: &str,
    enabled: bool,
    audit_every: Option<u32>,
    workers: usize,
    fork_seconds: &mut f64,
) -> Result<Vec<Trajectory>, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let fork_started = std::time::Instant::now();
    let policies = (0..workers.min(episodes.len()))
        .map(|_| policy.fork_for_collection())
        .collect::<Result<Vec<_>, _>>()?;
    *fork_seconds = fork_started.elapsed().as_secs_f64();
    let mut results = std::thread::scope(|scope| -> Result<Vec<(usize, Trajectory)>, String> {
        let mut handles = Vec::new();
        for (worker, mut policy) in policies.into_iter().enumerate() {
            let next = &next;
            handles.push(
                std::thread::Builder::new()
                    .name(format!("public-collector-{worker}"))
                    .stack_size(16 * 1024 * 1024)
                    .spawn_scoped(
                        scope,
                        move || -> Result<Vec<(usize, Trajectory)>, String> {
                            let mut completed = Vec::new();
                            loop {
                                let index = next.fetch_add(1, Ordering::Relaxed);
                                let Some(episode) = episodes.get(index) else {
                                    break;
                                };
                                eprintln!(
                                    "public collector {worker}: episode {index} {}",
                                    episode.id
                                );
                                completed.push((
                                    index,
                                    collect(
                                        &mut policy,
                                        episode,
                                        config_hash,
                                        state_hash,
                                        enabled,
                                        audit_every,
                                    )?,
                                ));
                            }
                            Ok(completed)
                        },
                    )
                    .map_err(err)?,
            );
        }
        let mut results = Vec::new();
        // Join every worker before returning an error. No collector survives
        // into an optimizer update or starts a speculative next batch.
        let mut failure = None;
        for handle in handles {
            match handle.join() {
                Ok(Ok(mut rows)) => results.append(&mut rows),
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    failure.get_or_insert("public collector panicked".into());
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(results)
    })?;
    results.sort_by_key(|(index, _)| *index);
    ensure(
        results.len() == episodes.len()
            && results
                .iter()
                .enumerate()
                .all(|(i, (index, _))| i == *index),
        "parallel public collection lost or duplicated an episode",
    )?;
    Ok(results
        .into_iter()
        .map(|(_, trajectory)| trajectory)
        .collect())
}

pub fn run(command: Command) -> Result<Value, String> {
    let config = &command.config;
    validate_entropy(config)?;
    let execution_gpu_ordinal = command.execution_gpu_ordinal.unwrap_or(config.gpu_ordinal);
    ensure(
        execution_gpu_ordinal < 16,
        "execution GPU ordinal outside bounds",
    )?;
    ensure(
        (1..=64).contains(&command.collector_workers),
        "public collector count outside bounds",
    )?;
    ensure(
        !config.updates.is_empty() && config.updates.len() <= 256,
        "public update count outside bounds",
    )?;
    ensure(
        config.learning_rate.to_bits() == 0.0001f32.to_bits()
            && config.value_coefficient.to_bits() == 0.5f32.to_bits()
            && config.gamma.to_bits() == 1f32.to_bits()
            && config.lambda.to_bits() == 0.9f32.to_bits(),
        "public learning uses fixed g115 GAE/loss scalars",
    )?;
    ensure(
        config.gpu_ordinal == 1 && (1..=512).contains(&config.max_chunk_substeps),
        "public GPU or chunk bound differs",
    )?;
    let mut ids = BTreeSet::new();
    for episodes in &config.updates {
        ensure(
            !episodes.is_empty() && episodes.len() <= 64,
            "public episode batch outside bounds",
        )?;
        for episode in episodes {
            episode.configurations_for_public_collector_v1()?;
            ensure(
                (episode.opponent.is_some() || episode.opponent_kind.is_some())
                    && ids.insert(episode.id.clone()),
                "missing opponent or duplicate episode",
            )?;
        }
    }
    let config_hash = sha(&serde_json::to_vec(config).map_err(err)?);
    let (base, state) = initialize(&config.source)?;
    ensure(
        base.feature_identity_v1().generation == FreshLineageGenerationV1::V4,
        "public initial source must be V4",
    )?;
    let initial_step = state.adam_step_v1();
    let (mut legacy, mut public, first_update) = if let Some(pin) = &command.resume {
        let checkpoint: Checkpoint =
            serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        ensure(
            checkpoint.schema == "mtg-kernel-public-input-checkpoint/v1"
                && checkpoint.config_sha256 == config_hash
                && checkpoint.optimizer_file == "optimizer.json",
            "public resume identity differs",
        )?;
        let path = pin
            .path
            .parent()
            .ok_or("resume path has no parent")?
            .join("optimizer.json");
        let saved = read_pinned_bytes(&PinnedFileV1 {
            path,
            sha256: checkpoint.optimizer_sha256,
        })?;
        let (legacy, public) = public_training::snapshot::decode(&saved).map_err(err)?;
        ensure(
            legacy.adam_step == initial_step + checkpoint.next_update as u64
                && public.adam_step == checkpoint.next_update as u64,
            "resume optimizer ages differ",
        )?;
        (legacy, public, checkpoint.next_update)
    } else {
        (
            state.snapshot_v1().map_err(err)?,
            ProjectionSnapshot::zero(),
            0,
        )
    };
    let last = command.stop_after.unwrap_or(config.updates.len());
    validate_projection_mode(config, &public)?;
    ensure(
        first_update < last && last <= config.updates.len(),
        "invalid public start/stop iteration",
    )?;
    let mut policy = PublicInputPlayPolicyV1::new(base, weights(&public)?)?
        .with_inputs_enabled(config.inputs_enabled);
    policy.install(&legacy.parameters, weights(&public)?)?;
    let mut device = PublicDeviceTrainState::import(
        &legacy,
        &public,
        &burn_cuda::CudaDevice::new(execution_gpu_ordinal),
    )
    .map_err(err)?;
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "config.json", config)?;
    search_opponent::reset_audit_counts();
    if let Some(receipt) = search_opponent::run_receipt(config, &config_hash, first_update..last)? {
        publish_json(
            &command.output_directory,
            "search-opponent-receipt.json",
            &receipt,
        )?;
    }
    if let Some(receipt) = opponent_kind::run_receipt(config, &config_hash)? {
        publish_json(
            &command.output_directory,
            "opponent-kinds-receipt.json",
            &receipt,
        )?;
    }
    let mut receipts = Vec::new();
    for update in first_update..last {
        let started = std::time::Instant::now();
        // Observational host wall times only. CUDA work may finish at a later
        // synchronization boundary, so these are not GPU kernel timings.
        // This map never enters the config, trajectory, optimizer or checkpoint.
        let mut stage_seconds = std::collections::BTreeMap::new();
        let directory = command.output_directory.join(format!("{update:04}"));
        fs::create_dir(&directory).map_err(err)?;
        let before_bytes = public_training::snapshot::encode(&legacy, &public).map_err(err)?;
        let before = sha(&before_bytes);
        stage_seconds.insert("prepare_and_before_state", started.elapsed().as_secs_f64());
        let mut trajectories = Vec::new();
        let mut trajectory_hashes = Vec::new();
        let mut fork_seconds = 0.0;
        let mut rollout_seconds = 0.0;
        let mut trajectory_publish_seconds = 0.0;
        if command.collector_workers > 1 {
            let collection_started = std::time::Instant::now();
            trajectories = collect_parallel(
                &policy,
                &config.updates[update],
                &config_hash,
                &before,
                config.inputs_enabled,
                command.search_boundary_audit_every,
                command.collector_workers,
                &mut fork_seconds,
            )
            .map_err(|e| search_opponent::publish_failure(&directory, e))?;
            rollout_seconds = collection_started.elapsed().as_secs_f64() - fork_seconds;
            let publish_started = std::time::Instant::now();
            for (index, trajectory) in trajectories.iter().enumerate() {
                trajectory_hashes.push(
                    publish_json(&directory, &format!("episode-{index:03}.json"), trajectory)?
                        .sha256,
                );
            }
            trajectory_publish_seconds = publish_started.elapsed().as_secs_f64();
        } else {
            for (index, episode) in config.updates[update].iter().enumerate() {
                let collection_started = std::time::Instant::now();
                eprintln!(
                    "public update {update}: collect {}/{} {}",
                    index + 1,
                    config.updates[update].len(),
                    episode.id
                );
                let trajectory = collect(
                    &mut policy,
                    episode,
                    &config_hash,
                    &before,
                    config.inputs_enabled,
                    command.search_boundary_audit_every,
                )
                .map_err(|e| search_opponent::publish_failure(&directory, e))?;
                rollout_seconds += collection_started.elapsed().as_secs_f64();
                let publish_started = std::time::Instant::now();
                trajectory_hashes.push(
                    publish_json(&directory, &format!("episode-{index:03}.json"), &trajectory)?
                        .sha256,
                );
                trajectories.push(trajectory);
                trajectory_publish_seconds += publish_started.elapsed().as_secs_f64();
            }
        }
        let collection_seconds = started.elapsed().as_secs_f64();
        stage_seconds.insert("collector_forks", fork_seconds);
        stage_seconds.insert("rollout_and_join", rollout_seconds);
        stage_seconds.insert("trajectory_publication", trajectory_publish_seconds);
        let replay_started = std::time::Instant::now();
        let mut records: Vec<
            Vec<(
                &DecisionRecordV1,
                NativeFlatDecisionTensorV4,
                &PublicFeatureRowsV1,
            )>,
        > = Vec::new();
        let mut raw_advantages = Vec::new();
        let mut value_targets = Vec::new();
        for trajectory in &trajectories {
            let group_start = records.len();
            for group in learner_groups(trajectory)? {
                for (row, tensor, auxiliary) in &group {
                    let output = policy.replay(tensor, auxiliary)?;
                    ensure(
                        bits(&output.logits) == row.logits && output.value.to_bits() == row.value,
                        "public rollout replay differs from current learner",
                    )?;
                }
                records.push(group);
            }
            ensure(
                records.len() > group_start,
                "episode has no learner physical decisions",
            )?;
            let values: Vec<_> = records[group_start..]
                .iter()
                .enumerate()
                .map(|(i, g)| {
                    (
                        f32::from_bits(g[0].0.value),
                        group_start + i == records.len() - 1,
                    )
                })
                .collect();
            let targets = gae_episode_advantages_v1(
                &values,
                trajectory.terminal.terminal_reward[trajectory.episode.learner_seat as usize]
                    as f32,
                config.gamma,
                config.lambda,
            )?;
            for (advantage, target) in targets {
                raw_advantages.push(advantage);
                value_targets.push(target);
            }
        }
        let (advantages, statistics) = normalize_advantages_v1(&raw_advantages, 1e-6)?;
        let steps: Vec<Vec<_>> = records
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|(row, tensor, auxiliary)| PublicTrainingStep {
                        view: encoded_decision_view_v4(tensor),
                        auxiliary,
                        selected: row.selected as usize,
                        expected_logits: &row.logits,
                        expected_value: row.value,
                    })
                    .collect()
            })
            .collect();
        let groups: Vec<_> = steps
            .iter()
            .map(|substeps| PublicTrainingGroup { substeps })
            .collect();
        stage_seconds.insert(
            "replay_targets_and_grouping",
            replay_started.elapsed().as_secs_f64(),
        );
        let update_started = std::time::Instant::now();
        device
            .update_groups(
                &groups,
                &value_targets,
                &advantages,
                config.learning_rate,
                config.value_coefficient,
                config.entropy_coefficient,
                config.inputs_enabled,
                config.projection_mode == ProjectionMode::All,
                config.max_chunk_substeps,
            )
            .map_err(err)?;
        stage_seconds.insert("device_update_call", update_started.elapsed().as_secs_f64());
        let snapshot_started = std::time::Instant::now();
        (legacy, public) = device.snapshot().map_err(err)?;
        stage_seconds.insert(
            "device_snapshot_call",
            snapshot_started.elapsed().as_secs_f64(),
        );
        let install_started = std::time::Instant::now();
        validate_projection_mode(config, &public)?;
        ensure(
            legacy.adam_step == initial_step + update as u64 + 1
                && public.adam_step == update as u64 + 1,
            "updated optimizer ages differ",
        )?;
        if !config.inputs_enabled {
            ensure(
                public
                    .object
                    .iter()
                    .chain(&public.state)
                    .chain(&public.object_first)
                    .chain(&public.object_second)
                    .chain(&public.state_first)
                    .chain(&public.state_second)
                    .all(|v| f32::from_bits(*v) == 0.0),
                "zero-input control acquired public weights or moments",
            )?;
        }
        policy.install(&legacy.parameters, weights(&public)?)?;
        stage_seconds.insert(
            "validate_and_install",
            install_started.elapsed().as_secs_f64(),
        );
        let encode_started = std::time::Instant::now();
        let optimizer = public_training::snapshot::encode(&legacy, &public).map_err(err)?;
        stage_seconds.insert("optimizer_encoding", encode_started.elapsed().as_secs_f64());
        let publish_started = std::time::Instant::now();
        let optimizer_hash = publish_bytes(&directory, "optimizer.json", &optimizer)?;
        stage_seconds.insert(
            "optimizer_publication",
            publish_started.elapsed().as_secs_f64(),
        );
        let checkpoint_started = std::time::Instant::now();
        let checkpoint = Checkpoint {
            schema: "mtg-kernel-public-input-checkpoint/v1".into(),
            config_sha256: config_hash.clone(),
            next_update: update + 1,
            optimizer_file: "optimizer.json".into(),
            optimizer_sha256: optimizer_hash.clone(),
            trajectory_sha256: trajectory_hashes,
        };
        publish_json(&directory, "checkpoint.json", &checkpoint)?;
        stage_seconds.insert(
            "checkpoint_publication",
            checkpoint_started.elapsed().as_secs_f64(),
        );
        let receipt = json!({"update":update,"execution_gpu_ordinal":execution_gpu_ordinal,"collector_workers":command.collector_workers,"episodes":trajectories.len(),"natural_games":trajectories.len(),"learner_groups":groups.len(),"learner_substeps":steps.iter().map(Vec::len).sum::<usize>(),
            "physical_decisions":trajectories.iter().map(|t|t.terminal.physical_decision_count).sum::<u64>(),"before_state_sha256":before,"after_state_sha256":optimizer_hash,
            "legacy_adam_step":legacy.adam_step,"public_adam_step":public.adam_step,"advantage_statistics":statistics,"collection_seconds":collection_seconds,"seconds":started.elapsed().as_secs_f64(),
            "stage_seconds":stage_seconds,"stage_timing_semantics":"host_wall/v1; CUDA calls may synchronize later; receipt publication excluded"});
        publish_json(&directory, "receipt.json", &receipt)?;
        receipts.push(receipt);
    }
    let result = json!({"schema":"mtg-kernel-public-input-run/v1","execution_gpu_ordinal":execution_gpu_ordinal,"collector_workers":command.collector_workers,"config_sha256":config_hash,"first_update":first_update,"next_update":last,"receipts":receipts,
        "non_claim":"Bounded local continuation only; no evaluation, promotion or human-strength claim."});
    if let Some(every) = command.search_boundary_audit_every {
        publish_json(
            &command.output_directory,
            "search-opponent-audit.json",
            &search_opponent::audit_report(every)?,
        )?;
    }
    publish_json(&command.output_directory, "completion.json", &result)?;
    Ok(result)
}
