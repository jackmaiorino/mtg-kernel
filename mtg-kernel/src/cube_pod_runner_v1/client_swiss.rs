//! Three-round single-game Swiss for human pods. The server owns this state and
//! advances it only after all four results in a round are natural.

use super::{pod_play_seed, PodRunnerError, PodUse, CLIENT_NAMESPACE};
use crate::cube_draft_v1::{derive_seed, game_seed, SeedStream, SEATS};
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
    pod_seed: u64,
    round: u8,
    points: [u8; SEATS],
    plays: [u8; SEATS],
    opponents: [u8; SEATS],
}

impl ClientSwiss {
    pub const NAMESPACE: &'static str = CLIENT_NAMESPACE;
    pub const USE: PodUse = PodUse::Client;

    pub fn new(run: &str, pod_index: u64) -> Result<Self, PodRunnerError> {
        Ok(Self {
            pod_seed: pod_play_seed(Self::NAMESPACE, run, pod_index)?,
            round: 0,
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
}
