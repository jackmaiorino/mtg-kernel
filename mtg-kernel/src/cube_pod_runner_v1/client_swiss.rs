//! Three-round single-game Swiss for human pods. The server owns this state and
//! advances it only after all four results in a round are natural.

use super::{
    execute_game_tasks_with_options, pod_play_seed, AttemptTiming, ExecOptions, GameRole, GameTask,
    PodCaps, PodGameRecord, PodRecordVersion, PodRunnerError, PodUse, SeatRule, CLIENT_NAMESPACE,
};
use crate::cube_draft_v1::{derive_seed, game_seed, Deck, SeedStream, SEATS};
use crate::cube_seat_v1::GamePilot;
use serde::Serialize;

pub const CLIENT_SWISS_ROUNDS: u8 = 3;
pub const CLIENT_SWISS_TABLES: usize = SEATS / 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SwissTable {
    /// Physical seats in play, draw order.
    pub chairs: [usize; 2],
    pub game: u32,
    pub game_seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SwissRound {
    pub round: u8,
    pub tables: [SwissTable; CLIENT_SWISS_TABLES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwissResult {
    Win { chair: usize },
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SwissStanding {
    pub chair: usize,
    pub points: u8,
}

pub struct ClientSwiss {
    run: String,
    pod_index: u64,
    pod_seed: u64,
    round: u8,
    bot_tables_started: bool,
    points: [u8; SEATS],
    plays: [u8; SEATS],
    opponents: [u8; SEATS],
}

impl ClientSwiss {
    pub const NAMESPACE: &'static str = CLIENT_NAMESPACE;
    pub const USE: PodUse = PodUse::Client;

    pub fn new(run: &str, pod_index: u64) -> Result<Self, PodRunnerError> {
        Ok(Self {
            run: run.to_owned(),
            pod_index,
            pod_seed: pod_play_seed(Self::NAMESPACE, run, pod_index)?,
            round: 0,
            bot_tables_started: false,
            points: [0; SEATS],
            plays: [0; SEATS],
            opponents: [0; SEATS],
        })
    }

    pub fn standings(&self) -> [SwissStanding; SEATS] {
        let mut standings = std::array::from_fn(|chair| SwissStanding {
            chair,
            points: self.points[chair],
        });
        standings
            .sort_unstable_by_key(|standing| (std::cmp::Reverse(standing.points), standing.chair));
        standings
    }

    /// Pair by minimum total points difference, excluding previous opponents.
    /// Ties use the first pairing in standings order. A play/draw imbalance
    /// is corrected first, with the runner's seat coin breaking ties.
    pub fn next_round(&self) -> Result<Option<SwissRound>, PodRunnerError> {
        if self.round == CLIENT_SWISS_ROUNDS {
            return Ok(None);
        }
        let ordered = self.standings().map(|standing| standing.chair);
        let mut best = None;
        self.find_matching(&ordered, &mut Vec::new(), 0, &mut best);
        let (_, pairing) = best.ok_or_else(|| invalid("no rematch-free Swiss pairing"))?;
        Ok(Some(SwissRound {
            round: self.round,
            tables: std::array::from_fn(|table| {
                let mut chairs = pairing[table];
                let game = u32::from(self.round) * CLIENT_SWISS_TABLES as u32 + table as u32;
                let seed = game_seed(self.pod_seed, game as usize);
                let first_plays = self.plays[chairs[0]];
                let second_plays = self.plays[chairs[1]];
                let coin = derive_seed(seed, SeedStream::GameCoin, 0) & 1;
                if first_plays > second_plays || (first_plays == second_plays && coin != 0) {
                    chairs.swap(0, 1);
                }
                SwissTable {
                    chairs,
                    game,
                    game_seed: seed,
                }
            }),
        }))
    }

    /// Executes the three private bot tables using the normal runner's A2
    /// caps and attempt-seed ledger. The human table is played by the
    /// fixed-seat human service. Three retries are available for an ordinary
    /// bot halt. A watchdog expiry stops the round. Dispatch is single-use
    /// even if execution or the completion hook fails.
    pub fn play_bot_tables(
        &mut self,
        round: &SwissRound,
        human_chair: usize,
        decks: &[Deck; SEATS],
        pilot: &(dyn GamePilot + Sync),
        on_game_complete: &(impl Fn(&PodGameRecord, &[AttemptTiming]) -> Result<(), String> + Sync),
    ) -> Result<Vec<PodGameRecord>, PodRunnerError> {
        if human_chair >= SEATS || self.next_round()?.as_ref() != Some(round) {
            return Err(invalid("invalid chair or stale Swiss round"));
        }
        if self.bot_tables_started {
            return Err(invalid(
                "bot tables already dispatched for this Swiss round",
            ));
        }
        self.bot_tables_started = true;
        let tasks: Vec<_> = round
            .tables
            .iter()
            .filter(|table| !table.chairs.contains(&human_chair))
            .map(|table| GameTask {
                game: table.game,
                group: u32::from(round.round),
                role: GameRole::Reference,
                pairing: None,
                decks: table.chairs.map(|chair| chair as u32),
                chairs: table.chairs.map(|chair| Some(chair as u8)),
                seats: table.chairs,
                arms: ["bot".into(), "bot".into()],
                seat_rule: SeatRule::Schedule,
                game_seed: table.game_seed,
                episode: u64::from(table.game),
            })
            .collect();
        let options = ExecOptions {
            namespace: CLIENT_NAMESPACE,
            run: &self.run,
            pod_index: self.pod_index,
            caps: PodCaps::training_a2(),
            watchdog_override: None,
            workers: tasks.len(),
            host: "local",
            stop_on_watchdog: true,
            record_version: PodRecordVersion::V3,
            pilot_sample_denom: 0,
            on_game_complete: Some(on_game_complete),
        };
        let results = execute_game_tasks_with_options(&tasks, decks, pilot, &options)?;
        if results.games.len() != CLIENT_SWISS_TABLES - 1
            || results.games.iter().any(|game| game.scores.is_none())
        {
            return Err(invalid("a bot table stopped without a natural result"));
        }
        Ok(results.games)
    }

    /// Commit all four tables together. Reject stale rounds and winners
    /// outside their own table before changing standings.
    pub fn finish_round(
        &mut self,
        round: &SwissRound,
        results: [SwissResult; CLIENT_SWISS_TABLES],
    ) -> Result<(), PodRunnerError> {
        if self.next_round()?.as_ref() != Some(round) {
            return Err(invalid("round differs from the current Swiss schedule"));
        }
        let mut points = self.points;
        for (table, result) in round.tables.iter().zip(results) {
            let [first, second] = table.chairs;
            match result {
                SwissResult::Win { chair } if table.chairs.contains(&chair) => points[chair] += 3,
                SwissResult::Win { .. } => {
                    return Err(invalid("winner was not seated at this table"))
                }
                SwissResult::Draw => {
                    points[first] += 1;
                    points[second] += 1;
                }
            }
        }
        self.points = points;
        for table in &round.tables {
            let [first, second] = table.chairs;
            self.plays[first] += 1;
            self.opponents[first] |= 1 << second;
            self.opponents[second] |= 1 << first;
        }
        self.round += 1;
        self.bot_tables_started = false;
        Ok(())
    }

    fn find_matching(
        &self,
        remaining: &[usize],
        pairs: &mut Vec<[usize; 2]>,
        gap: u8,
        best: &mut Option<(u8, [[usize; 2]; CLIENT_SWISS_TABLES])>,
    ) {
        if best.as_ref().is_some_and(|(best_gap, _)| gap >= *best_gap) {
            return;
        }
        let Some((&first, rest)) = remaining.split_first() else {
            *best = Some((gap, std::array::from_fn(|i| pairs[i])));
            return;
        };
        for (index, &other) in rest.iter().enumerate() {
            if self.opponents[first] & (1 << other) != 0 {
                continue;
            }
            let mut still_unpaired = [0; SEATS - 2];
            let unpaired = &mut still_unpaired[..rest.len() - 1];
            unpaired[..index].copy_from_slice(&rest[..index]);
            unpaired[index..].copy_from_slice(&rest[index + 1..]);
            pairs.push([first, other]);
            self.find_matching(
                unpaired,
                pairs,
                gap + self.points[first].abs_diff(self.points[other]),
                best,
            );
            pairs.pop();
        }
    }
}

fn invalid(message: &str) -> PodRunnerError {
    PodRunnerError::InvalidPlan(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn plays_three_disjoint_rounds_and_pairs_winners_after_round_one() {
        let mut swiss = ClientSwiss::new("fixture", 0).unwrap();
        assert_eq!(ClientSwiss::NAMESPACE, "client-pods");
        assert_eq!(ClientSwiss::USE, PodUse::Client);
        let mut opponents = BTreeSet::new();
        let mut seeds = BTreeSet::new();

        for round_index in 0..CLIENT_SWISS_ROUNDS {
            let round = swiss.next_round().unwrap().unwrap();
            assert_eq!(round.round, round_index);
            let mut chairs = BTreeSet::new();
            for table in round.tables {
                let [first, second] = table.chairs;
                assert!(chairs.insert(first) && chairs.insert(second));
                assert!(opponents.insert([first.min(second), first.max(second)]));
                assert!(seeds.insert(table.game_seed));
                if round_index == 1 {
                    assert_eq!(first % 2, second % 2);
                }
            }
            let results = round.tables.map(|table| {
                if round_index == 0 {
                    SwissResult::Win {
                        chair: table.chairs[0].min(table.chairs[1]),
                    }
                } else {
                    SwissResult::Draw
                }
            });
            swiss.finish_round(&round, results).unwrap();
            assert!(swiss.finish_round(&round, results).is_err());
        }
        assert!(swiss.next_round().unwrap().is_none());
        assert_eq!(opponents.len(), 12);
        assert_eq!(
            swiss.standings().map(|standing| standing.points),
            [5, 5, 5, 5, 2, 2, 2, 2]
        );
    }

    #[test]
    fn invalid_winner_leaves_the_round_uncommitted() {
        let mut swiss = ClientSwiss::new("fixture", 0).unwrap();
        let round = swiss.next_round().unwrap().unwrap();
        let mut results = [SwissResult::Draw; CLIENT_SWISS_TABLES];
        results[3] = SwissResult::Win {
            chair: round.tables[0].chairs[0],
        };
        assert!(swiss.finish_round(&round, results).is_err());
        assert_eq!(swiss.next_round().unwrap(), Some(round));
        assert!(swiss.standings().iter().all(|row| row.points == 0));
    }

    #[test]
    fn runner_excludes_human_table_and_journals_bot_replays() {
        use super::super::attempt_seeds;
        use crate::cube_draft_v1::{GameCaps, GameEnd};
        use crate::cube_seat_v1::{AttemptOutcome, AttemptSpec};
        use std::sync::Mutex;

        struct RetryPilot([u64; CLIENT_SWISS_TABLES]);
        impl GamePilot for RetryPilot {
            fn play(&self, spec: &AttemptSpec<'_>, _: &GameCaps) -> AttemptOutcome {
                let end = if self.0.contains(&spec.env_seed) {
                    GameEnd::Terminal {
                        code: "fixture".into(),
                        classification: "fixture".into(),
                        reason: "fixture".into(),
                    }
                } else {
                    GameEnd::Natural {
                        winner_seat: Some(spec.seats[0]),
                        reason: "fixture".into(),
                    }
                };
                AttemptOutcome {
                    decisions: 1,
                    physical_decisions: Some(1),
                    policy_steps: Some(1),
                    end,
                    last_action_source: None,
                    first_player_seat: Some(spec.seats[0]),
                    last_acting_seat: Some(spec.seats[0]),
                    pilot_sample: None,
                }
            }
        }

        let mut swiss = ClientSwiss::new("fixture", 0).unwrap();
        let round = swiss.next_round().unwrap().unwrap();
        let expected: Vec<_> = round
            .tables
            .iter()
            .filter(|table| !table.chairs.contains(&0))
            .map(|table| (table.game, table.chairs.map(|chair| Some(chair as u8))))
            .collect();
        let pilot = RetryPilot(round.tables.map(|t| attempt_seeds(t.game_seed, 0).1));
        let decks = std::array::from_fn(|_| Deck::default());
        let observed = Mutex::new(Vec::new());
        let hook = |record: &PodGameRecord, _: &[AttemptTiming]| {
            observed.lock().unwrap().push(record.game);
            Ok(())
        };
        let games = swiss
            .play_bot_tables(&round, 0, &decks, &pilot, &hook)
            .unwrap();
        assert_eq!(expected.len(), 3);
        let actual: Vec<_> = games.iter().map(|game| (game.game, game.chairs)).collect();
        assert_eq!(actual, expected);
        assert!(swiss
            .play_bot_tables(&round, 0, &decks, &pilot, &hook)
            .is_err());
        for game in &games {
            assert_eq!(game.attempts.len(), 2);
            assert!(game.attempts[0].halted);
            assert!(!game.attempts[1].halted);
            assert_eq!(
                game.attempts[1].env_seed,
                attempt_seeds(game.game_seed, 1).1
            );
            assert_eq!(game.scores, Some([1, -1]));
        }
        let mut emitted = observed.into_inner().unwrap();
        emitted.sort_unstable();
        let expected_ids: Vec<_> = expected.iter().map(|(id, _)| *id).collect();
        assert_eq!(emitted, expected_ids);

        let mut failed_swiss = ClientSwiss::new("fixture", 0).unwrap();
        let failed_calls = Mutex::new(Vec::new());
        let failing_hook = |record: &PodGameRecord, _: &[AttemptTiming]| {
            failed_calls.lock().unwrap().push(record.game);
            Err("journal failure".into())
        };
        assert!(failed_swiss
            .play_bot_tables(&round, 0, &decks, &pilot, &failing_hook)
            .is_err());
        let after_failure = failed_calls.lock().unwrap().len();
        assert!(after_failure > 0);
        assert!(failed_swiss
            .play_bot_tables(&round, 0, &decks, &pilot, &failing_hook)
            .is_err());
        assert_eq!(failed_calls.lock().unwrap().len(), after_failure);
    }
}
