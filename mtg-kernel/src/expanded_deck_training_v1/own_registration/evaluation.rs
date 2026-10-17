//! CPU raw-policy evaluation of the versioned registration checkpoint.
//! Uses the same explicit-context collector, without importing a GPU device.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub config: PinnedFileV1,
    pub checkpoint: PinnedFileV1,
    pub episodes: PinnedFileV1,
    pub output_directory: PathBuf,
    pub collector_workers: usize,
    #[serde(default)]
    pub capture_decisions: bool,
}

fn validate_panel(episodes: &[ExpandedEpisodeV1], workers: usize) -> Result<(), String> {
    ensure(
        !episodes.is_empty() && episodes.len() <= 16384,
        "registration evaluation needs 1..16384 episodes",
    )?;
    validate_collection_workers_v1(workers)?;
    let mut ids = BTreeSet::new();
    for episode in episodes {
        episode.configurations()?;
        ensure(
            ids.insert(&episode.id),
            "registration evaluation repeats an external episode id",
        )?;
        ensure(
            episode.opponent.is_some() && episode.reset.is_none(),
            "registration evaluation requires fresh games against pinned reference opponents",
        )?;
    }
    Ok(())
}

pub fn run(command: Command) -> Result<Value, String> {
    let episodes: Vec<ExpandedEpisodeV1> = read_pinned(&command.episodes)?;
    validate_panel(&episodes, command.collector_workers)?;
    let (policy, identity, mode, projection_enabled) =
        training::load_for_evaluation(&command.config, &command.checkpoint)?;
    // The loader has validated this exact checkpoint and its optimizer pin.
    let checkpoint: Value = read_pinned(&command.checkpoint)?;
    let optimizer_sha256 = checkpoint["optimizer_sha256"]
        .as_str()
        .ok_or("registration evaluation checkpoint has no optimizer identity")?;
    // Worker count and output placement must not change the primary result.
    let evaluation_sha256 = sha(&serde_json::to_vec(&(
        "own-registration-evaluation-input/v1",
        &command.config.sha256,
        &command.checkpoint.sha256,
        &command.episodes.sha256,
        command.capture_decisions,
    ))
    .map_err(err)?);
    ensure(
        !command.output_directory.exists(),
        "registration evaluation requires a fresh output directory",
    )?;
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "command.json", &command)?;
    publish_json(&command.output_directory, "model.json", &identity)?;
    let started = std::time::Instant::now();
    let mut games = Vec::with_capacity(episodes.len());
    let mut totals = [0usize; 3];
    // Bound retained tensor memory to one worker-sized wave. Every wave uses
    // the same frozen weights, and publication follows the input schedule.
    for (wave, batch) in episodes.chunks(command.collector_workers).enumerate() {
        let trajectories = match collect_parallel(
            &policy,
            batch,
            &evaluation_sha256,
            optimizer_sha256,
            mode,
            projection_enabled,
            command.collector_workers,
        ) {
            Ok(rows) => rows,
            Err(error) => {
                publish_json(
                    &command.output_directory,
                    "failure.json",
                    &json!({
                        "schema":"own-registration-evaluation-failure/v1",
                        "evaluation_sha256":evaluation_sha256,
                        "completed_games":games.len(),
                        "failed_wave_episode_ids":batch.iter().map(|e| &e.id).collect::<Vec<_>>(),
                        "error":error,
                    }),
                )?;
                return Err(error);
            }
        };
        ensure(
            trajectories.len() == batch.len(),
            "evaluation collector omitted an episode",
        )?;
        for (within_wave, trajectory) in trajectories.iter().enumerate() {
            trajectory.validate_records()?;
            let index = wave * command.collector_workers + within_wave;
            ensure(
                trajectory.episode.id == episodes[index].id,
                "registration evaluation changed schedule order",
            )?;
            let reward =
                trajectory.terminal.terminal_reward[trajectory.episode.learner_seat as usize];
            let outcome_index = match reward {
                1 => 0,
                0 => 1,
                -1 => 2,
                _ => return Err("registration evaluation invalid terminal reward".into()),
            };
            totals[outcome_index] += 1;
            let trajectory_pin = if command.capture_decisions {
                Some(
                    publish_json(
                        &command.output_directory,
                        &format!("trajectory-{index:05}.json"),
                        trajectory,
                    )?
                    .sha256,
                )
            } else {
                None
            };
            let game = json!({
                "schema":"own-registration-evaluation-game/v1",
                "index":index,
                "episode_id":trajectory.episode.id,
                "seed":trajectory.episode.seed,
                "learner_seat":trajectory.episode.learner_seat,
                "starting_player":trajectory.episode.starting_player,
                "configuration_sha256":trajectory.configuration_sha256,
                "opponent":trajectory.opponent,
                "terminal":trajectory.terminal,
                "learner_reward":reward,
                "trajectory_sha256":trajectory_pin,
            });
            let pin = publish_json(
                &command.output_directory,
                &format!("game-{index:05}.json"),
                &game,
            )?;
            games.push(pin.sha256);
        }
    }
    let receipt = json!({
        "schema":"own-registration-evaluation-completion/v1",
        "evaluation_sha256":evaluation_sha256,
        "model":identity,
        "games":games.len(),
        "wins":totals[0],"draws":totals[1],"losses":totals[2],
        "game_sha256":games,
        "natural_terminal_only":true,
    });
    // Timings do not enter the deterministic primary output store.
    publish_json(
        &command.output_directory,
        "timing.json",
        &json!({"wall_seconds":started.elapsed().as_secs_f64(),"collector_workers":command.collector_workers}),
    )?;
    publish_json(&command.output_directory, "completion.json", &receipt)?;
    Ok(receipt)
}
