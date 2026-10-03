//! CPU-only verification of complete captured learner behavior, without sampling.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub config: PinnedFileV1,
    pub checkpoint: PinnedFileV1,
    pub trajectories: Vec<PinnedFileV1>,
    pub output_directory: PathBuf,
}

pub fn run(command: Command) -> Result<Value, String> {
    ensure(
        (1..=12).contains(&command.trajectories.len()),
        "stack replay archive count outside bounds",
    )?;
    let (policy, identity) = load_for_evaluation(&command.config, &command.checkpoint)?;
    let mut reports = Vec::new();
    let mut total = 0;
    for pin in &command.trajectories {
        let trajectory: Trajectory =
            serde_json::from_slice(&read_pinned_bytes(pin)?).map_err(err)?;
        ensure(
            trajectory.schema == "mtg-kernel-public-stack-trajectory/v1"
                && trajectory.config_sha256
                    == identity["config_sha256"]
                        .as_str()
                        .ok_or("missing config identity")?
                && trajectory.optimizer_state_sha256
                    == identity["optimizer_sha256"]
                        .as_str()
                        .ok_or("missing optimizer identity")?
                && serde_json::to_value(trajectory.input_mode).map_err(err)?
                    == identity["input_mode"]
                && trajectory.decisions.len() == trajectory.auxiliary.len(),
            "stack behavior archive identity differs from loaded model",
        )?;
        validate_episode_records_v1(
            &trajectory.episode,
            &trajectory.configuration_sha256,
            &trajectory.decisions,
            &trajectory.terminal,
        )?;
        let mut count = 0;
        let mut choices = 0;
        for (row, auxiliary) in trajectory.decisions.iter().zip(&trajectory.auxiliary) {
            if row.actor != trajectory.episode.learner_seat {
                ensure(
                    auxiliary.is_none(),
                    "opponent carries learner stack capture",
                )?;
                continue;
            }
            let captured = StackEncodedDecisionV1 {
                legacy: NativeFlatDecisionTensorV4 {
                    common: row.tensor.tensor(),
                },
                stack: auxiliary
                    .as_ref()
                    .ok_or("learner stack capture missing")?
                    .clone(),
            };
            let output = policy.replay(&captured)?;
            ensure(
                bits(&output.logits) == row.logits && output.value.to_bits() == row.value,
                "loaded stack policy differs from archived behavior",
            )?;
            count += 1;
            choices += usize::from(row.logits.len() > 1);
        }
        ensure(
            count > 0 && choices > 0,
            "stack archive has no learner choices",
        )?;
        total += count;
        reports.push(
            json!({"trajectory":pin,"learner_seat":trajectory.episode.learner_seat,
            "exact_behavior_rows":count,"choice_rows":choices}),
        );
    }
    let result = json!({"schema":"public-stack-behavior-replay/v1","model":identity,
        "archives":reports,"exact_behavior_rows":total,
        "non_claim":"Complete archived learner score replay only; no new games, sampler draws or strength inference."});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "result.json", &result)?;
    Ok(result)
}
