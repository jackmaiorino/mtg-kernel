//! D4 fixture part 3 through the real D5 retry and stop paths.

use super::*;
use crate::cube_draft_build_v1::model::training::fixture::{halt_cases, HaltCase};
use crate::cube_draft_v1::GameEnd;
use crate::cube_pod_runner_v1::{
    collect_halts_and_stop, play_training_game_for_driver, GameRole, SeatRule, POD_RECORD_SCHEMA,
};
use crate::cube_seat_v1::{AttemptOutcome, AttemptSpec};

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
        let attempt = (0..=3u8)
            .find(|attempt| {
                crate::cube_pod_runner_v1::attempt_seeds(self.game_seed, *attempt).2
                    == spec.action_seed
            })
            .expect("two-action attempt seed");
        let halt = if self.deterministic_halt {
            self.case.deterministic_halt()
        } else {
            self.case.halts(usize::from(attempt))
        };
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
    deterministic_halt: bool,
) -> Result<TwoActionPart3Report, TrainingDriverError> {
    fs::create_dir_all(root)?;
    let decks = [
        crate::cube_draft_v1::Deck::default(),
        crate::cube_draft_v1::Deck::default(),
    ];
    let mut derivative_sum = 0.0;
    let mut published = 0usize;
    let mut stopped = 0usize;
    for (index, case) in halt_cases().enumerate() {
        if deterministic_halt && case.pattern != 15 {
            continue;
        }
        let game_seed = index as u64;
        let pilot = TwoActionPilot {
            case,
            deterministic_halt,
            game_seed,
        };
        let task = crate::cube_pod_runner_v1::GameTask {
            game: 0,
            group: 0,
            role: GameRole::Reference,
            pairing: Some(0),
            decks: [0, 1],
            chairs: [Some(0), Some(1)],
            seats: [0, 1],
            seat_rule: SeatRule::Schedule,
            game_seed,
            episode: 0,
        };
        let caps = crate::cube_pod_runner_v1::PodCaps::training_a2();
        let game = play_training_game_for_driver(&task, &decks, &pilot, &caps)?;
        let (halts, status, stop) = collect_halts_and_stop(std::slice::from_ref(&game), true);
        let pod = PodRecord {
            schema: POD_RECORD_SCHEMA,
            interfaces: M4_INTERFACES_VERSION,
            pod_use: PodUse::Test,
            namespace: "test".to_owned(),
            run: "v1".to_owned(),
            pod_index: 0,
            seeds: Default::default(),
            pins: PodPins::default(),
            pilot: PilotInfo::uniform(),
            caps,
            draft: None,
            chairs: Vec::new(),
            decks: Vec::new(),
            games: vec![game],
            paired_builds: Vec::new(),
            halts,
            status,
            stop,
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
