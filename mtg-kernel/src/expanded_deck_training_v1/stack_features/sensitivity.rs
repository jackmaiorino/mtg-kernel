//! Read-only projection sensitivity on exactly replayed archived behavior.
use super::*;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub config: PinnedFileV1,
    pub checkpoint: PinnedFileV1,
    pub trajectories: Vec<PinnedFileV1>,
    pub output_directory: PathBuf,
}

#[derive(Default, Serialize)]
struct Metrics {
    choices: usize,
    argmax_flips: usize,
    total_variation_sum: f64,
    total_variation_max: f64,
    absolute_logit_delta_max: f64,
    absolute_value_delta_max: f64,
}

fn probabilities(logits: &[f32]) -> Vec<f64> {
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    let mut values: Vec<_> = logits.iter().map(|v| (*v as f64 - maximum).exp()).collect();
    let sum: f64 = values.iter().sum();
    for v in &mut values {
        *v /= sum;
    }
    values
}

fn argmax(logits: &[f32]) -> usize {
    let mut best = 0;
    for i in 1..logits.len() {
        if logits[i] > logits[best] {
            best = i;
        }
    }
    best
}

impl Metrics {
    fn add(
        &mut self,
        full: &crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1,
        off: &crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1,
    ) {
        let left = probabilities(&full.logits);
        let right = probabilities(&off.logits);
        let tv = left
            .iter()
            .zip(right)
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 2.0;
        self.choices += 1;
        self.argmax_flips += usize::from(argmax(&full.logits) != argmax(&off.logits));
        self.total_variation_sum += tv;
        self.total_variation_max = self.total_variation_max.max(tv);
        self.absolute_value_delta_max = self
            .absolute_value_delta_max
            .max((full.value as f64 - off.value as f64).abs());
        for (a, b) in full.logits.iter().zip(&off.logits) {
            self.absolute_logit_delta_max = self
                .absolute_logit_delta_max
                .max((*a as f64 - *b as f64).abs());
        }
    }
}

pub fn run(command: Command) -> Result<Value, String> {
    ensure(
        (1..=10).contains(&command.trajectories.len()),
        "diagnostic archive count outside bounds",
    )?;
    let (policy, identity) = load_for_evaluation(&command.config, &command.checkpoint)?;
    ensure(
        identity["input_mode"] == "structured",
        "sensitivity requires structured behavior",
    )?;
    let off = policy.fork_for_collection()?.with_inputs_enabled(false);
    let mut groups: BTreeMap<String, Metrics> = BTreeMap::new();
    let mut archives = Vec::new();
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
                && trajectory.input_mode == StackInputModeV1::Structured
                && trajectory.decisions.len() == trajectory.auxiliary.len(),
            "diagnostic behavior identity differs",
        )?;
        validate_episode_records_v1(
            &trajectory.episode,
            &trajectory.configuration_sha256,
            &trajectory.decisions,
            &trajectory.terminal,
        )?;
        let mut exact = 0;
        for (row, auxiliary) in trajectory.decisions.iter().zip(&trajectory.auxiliary) {
            if row.actor != trajectory.episode.learner_seat {
                ensure(auxiliary.is_none(), "opponent carries learner auxiliary")?;
                continue;
            }
            let captured = StackEncodedDecisionV1 {
                legacy: NativeFlatDecisionTensorV4 {
                    common: row.tensor.tensor(),
                },
                stack: auxiliary
                    .as_ref()
                    .ok_or("learner auxiliary missing")?
                    .clone(),
            };
            let full = policy.replay(&captured)?;
            ensure(
                bits(&full.logits) == row.logits && full.value.to_bits() == row.value,
                "diagnostic exact behavior replay failed",
            )?;
            exact += 1;
            total += 1;
            ensure(total <= 100000, "diagnostic row cap exceeded")?;
            if full.logits.len() <= 1 {
                continue;
            }
            let ablated = off.replay(&captured)?;
            ensure(
                full.logits.len() == ablated.logits.len(),
                "ablation legal menu differs",
            )?;
            let empty = captured.stack.stack_items == 0;
            let special = captured
                .stack
                .rows
                .iter()
                .filter(|r| r.features[1] == 1.0)
                .any(|r| {
                    let f = &r.features;
                    f[13] == 1.0
                        || f[15] == 1.0
                        || f[25] == 0.0
                        || f[289..305].iter().any(|v| *v != 0.0)
                });
            if empty {
                ensure(
                    bits(&full.logits) == bits(&ablated.logits)
                        && full.value.to_bits() == ablated.value.to_bits(),
                    "empty-stack ablation changed output",
                )?;
            }
            let category = if empty {
                "empty"
            } else if special {
                "special"
            } else {
                "default"
            };
            groups.entry("all".into()).or_default().add(&full, &ablated);
            groups
                .entry(category.into())
                .or_default()
                .add(&full, &ablated);
        }
        ensure(exact > 0, "archive contains no learner decisions")?;
        archives.push(json!({"trajectory":pin,"exact_rows":exact}));
    }
    let result = json!({"schema":"public-stack-sensitivity/v1","model":identity,
        "archives":archives,"exact_behavior_rows":total,"groups":groups,
        "non_claim":"Read-only whole-message ablation on actual behavior states, not feature-specific utility or counterfactual gameplay."});
    fs::create_dir(&command.output_directory).map_err(err)?;
    publish_json(&command.output_directory, "result.json", &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn softmax_shift_and_argmax_tie_are_defined() {
        assert_eq!(probabilities(&[0., 0.]), vec![0.5, 0.5]);
        assert_eq!(probabilities(&[100., 100.]), vec![0.5, 0.5]);
        assert_eq!(argmax(&[2., 2., 1.]), 0);
        let p = probabilities(&[10000., -10000.]);
        assert_eq!(p, vec![1., 0.]);
    }
}
