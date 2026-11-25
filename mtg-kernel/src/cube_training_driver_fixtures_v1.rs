//! D4 fixture part 3 through the real D5 retry and stop paths.

use super::*;
use crate::cube_draft_build_v1::model::training::fixture::{halt_cases, HaltCase};
use crate::cube_draft_v1::{game_seed, GameEnd};
use crate::cube_pod_runner_v1::{
    attempt_seeds, pod_play_seed, run_training_pod, uniform_training_chairs, TrainingPodConfig,
};
use crate::cube_seat_v1::{AttemptOutcome, AttemptSpec};

/// Check the parameters bound to the current and cached league seats.
#[cfg(feature = "eternal-fixtures")]
pub fn assert_snapshot_parameter_digests(driver: &TrainingDriver) {
    for snapshot in std::iter::once(&driver.model).chain(driver.snapshots.values()) {
        let mut fresh = snapshot.model.as_ref().clone();
        fresh.visit_tensors_mut_v1(|_, _, _, _| {});
        assert_eq!(snapshot.model.parameter_sha256(), fresh.parameter_sha256());
    }
}

struct TwoActionPilot {
    case: HaltCase,
    deterministic_halt: bool,
    game_seed: u64,
}

impl GamePilot for TwoActionPilot {
    fn play(
        &self,
        spec: &AttemptSpec<'_>,
        _caps: &crate::cube_draft_v1::GameCaps,
    ) -> AttemptOutcome {
        let attempt =
            (0..=3u8).find(|attempt| attempt_seeds(self.game_seed, *attempt).2 == spec.action_seed);
        let halt = attempt.is_some_and(|attempt| {
            if self.deterministic_halt {
                self.case.deterministic_halt()
            } else {
                self.case.halts(usize::from(attempt))
            }
        });
        if halt {
            return AttemptOutcome {
                decisions: 1,
                physical_decisions: Some(1),
                policy_steps: Some(1),
                end: GameEnd::DecisionCap,
                last_action_source: None,
            };
        }
        let result = self.case.actions[0] as i8 - self.case.actions[1] as i8;
        AttemptOutcome {
            decisions: 1,
            physical_decisions: Some(1),
            policy_steps: Some(1),
            end: GameEnd::Natural {
                winner_seat: match result {
                    1 => Some(spec.seats[0]),
                    -1 => Some(spec.seats[1]),
                    _ => None,
                },
                reason: "fixture".to_owned(),
            },
            last_action_source: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TwoActionPart3Report {
    pub published_mean: f64,
    pub published_batches: usize,
    pub stopped_batches: usize,
}

pub fn run_two_action_part3(
    root: &Path,
    pool: &CubePool,
    deterministic_halt: bool,
) -> Result<TwoActionPart3Report, TrainingDriverError> {
    fs::create_dir_all(root)?;
    let roles = training_roles_with_older([0, 1])?;
    let mut chairs = uniform_training_chairs(roles);
    let mut derivative_sum = 0.0;
    let mut published = 0usize;
    let mut stopped = 0usize;
    for (index, case) in halt_cases().enumerate() {
        if deterministic_halt && case.pattern != 15 {
            continue;
        }
        let config = TrainingPodConfig::uniform_a2(PodUse::Test, "test", "v1", index as u64, 1);
        let pod_seed = pod_play_seed(&config.namespace, &config.run, config.pod_index)?;
        let pilot = TwoActionPilot {
            case,
            deterministic_halt,
            game_seed: game_seed(pod_seed, 0),
        };
        let pod = match run_training_pod(pool, &config, &mut chairs, &pilot) {
            Ok(pod) => pod,
            Err(PodRunnerError::Stopped(pod)) => *pod,
            Err(error) => return Err(error.into()),
        };
        if pod.status == PodStatus::Stopped {
            stopped += 1;
            let record = stop_from_failure(
                "v1",
                index as u64,
                BatchFailure {
                    pod: Box::new(pod),
                    completed: Vec::new(),
                },
            );
            ledger_stop(root, &record)?;
            if check_stop_ledger(root) != Err(TrainingDriverError::StoppedWithoutResolution) {
                return Err(invalid("a stop without resolution did not refuse resume"));
            }
        } else {
            published += 1;
            let expected = case.actions[0] as i8 - case.actions[1] as i8;
            if pod.games[0].scores != Some([expected, -expected]) {
                return Err(invalid("two-action replay changed the completed score"));
            }
            derivative_sum += case.derivative;
        }
    }
    let total = if deterministic_halt { 4.0 } else { 64.0 };
    let published_mass = published as f64 / total;
    Ok(TwoActionPart3Report {
        published_mean: if published == 0 {
            f64::NAN
        } else {
            derivative_sum / total / published_mass
        },
        published_batches: published,
        stopped_batches: stopped,
    })
}

pub fn two_action_dropped_mean() -> f64 {
    let (sum, count) = halt_cases()
        .filter(|case| !case.halts(0))
        .fold((0.0, 0), |(sum, count), case| {
            (sum + case.derivative, count + 1)
        });
    sum / f64::from(count)
}
