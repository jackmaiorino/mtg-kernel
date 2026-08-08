//! Deterministic best-of-three match state layered outside `GameState`.
//!
//! The match owns only cross-game facts. Each game remains an ordinary kernel
//! game and may use the existing explicit-starting-player constructors.

use crate::ids::PlayerId;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

pub const BEST_OF_THREE_MAX_GAMES_V1: u8 = 3;
pub const BEST_OF_THREE_WINS_REQUIRED_V1: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayDrawChoiceV1 {
    Play,
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameOutcomeV1 {
    Win { winner: PlayerId },
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchOutcomeV1 {
    Winner { winner: PlayerId },
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameStartV1 {
    pub game_index: u8,
    pub chooser: PlayerId,
    pub choice: PlayDrawChoiceV1,
    pub starting_player: PlayerId,
}

impl GameStartV1 {
    pub fn player_on_draw(self) -> PlayerId {
        self.starting_player.opponent()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedMatchGameV1 {
    pub start: GameStartV1,
    pub outcome: GameOutcomeV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPhaseV1 {
    AwaitingPlayDrawChoice { game_index: u8, chooser: PlayerId },
    AwaitingGameResult { start: GameStartV1 },
    Complete { outcome: MatchOutcomeV1 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchTransitionV1 {
    NextGameChoice { game_index: u8, chooser: PlayerId },
    Complete { outcome: MatchOutcomeV1 },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct BestOfThreeMatchStateV1 {
    wins: [u8; 2],
    games: Vec<CompletedMatchGameV1>,
    phase: MatchPhaseV1,
}

impl BestOfThreeMatchStateV1 {
    pub fn new_v1(game_one_chooser: PlayerId) -> Result<Self, MatchStateErrorV1> {
        validate_player_v1(game_one_chooser)?;
        Ok(Self {
            wins: [0, 0],
            games: Vec::with_capacity(usize::from(BEST_OF_THREE_MAX_GAMES_V1)),
            phase: MatchPhaseV1::AwaitingPlayDrawChoice {
                game_index: 1,
                chooser: game_one_chooser,
            },
        })
    }

    pub fn wins(&self, player: PlayerId) -> Result<u8, MatchStateErrorV1> {
        validate_player_v1(player)?;
        Ok(self.wins[player.index()])
    }

    pub fn games(&self) -> &[CompletedMatchGameV1] {
        &self.games
    }

    pub fn phase(&self) -> MatchPhaseV1 {
        self.phase
    }

    pub fn outcome(&self) -> Option<MatchOutcomeV1> {
        match self.phase {
            MatchPhaseV1::Complete { outcome } => Some(outcome),
            _ => None,
        }
    }

    pub fn choose_play_draw_v1(
        &mut self,
        chooser: PlayerId,
        choice: PlayDrawChoiceV1,
    ) -> Result<GameStartV1, MatchStateErrorV1> {
        validate_player_v1(chooser)?;
        let (game_index, expected_chooser) = match self.phase {
            MatchPhaseV1::AwaitingPlayDrawChoice {
                game_index,
                chooser,
            } => (game_index, chooser),
            MatchPhaseV1::AwaitingGameResult { .. } => {
                return Err(MatchStateErrorV1::GameAlreadyStarted)
            }
            MatchPhaseV1::Complete { .. } => return Err(MatchStateErrorV1::MatchComplete),
        };
        if chooser != expected_chooser {
            return Err(MatchStateErrorV1::WrongChooser {
                expected: expected_chooser,
                actual: chooser,
            });
        }
        let starting_player = match choice {
            PlayDrawChoiceV1::Play => chooser,
            PlayDrawChoiceV1::Draw => chooser.opponent(),
        };
        let start = GameStartV1 {
            game_index,
            chooser,
            choice,
            starting_player,
        };
        self.phase = MatchPhaseV1::AwaitingGameResult { start };
        Ok(start)
    }

    pub fn record_game_result_v1(
        &mut self,
        outcome: GameOutcomeV1,
    ) -> Result<MatchTransitionV1, MatchStateErrorV1> {
        let start = match self.phase {
            MatchPhaseV1::AwaitingGameResult { start } => start,
            MatchPhaseV1::AwaitingPlayDrawChoice { .. } => {
                return Err(MatchStateErrorV1::PlayDrawChoiceRequired)
            }
            MatchPhaseV1::Complete { .. } => return Err(MatchStateErrorV1::MatchComplete),
        };
        if let GameOutcomeV1::Win { winner } = outcome {
            validate_player_v1(winner)?;
            self.wins[winner.index()] += 1;
        }
        self.games.push(CompletedMatchGameV1 { start, outcome });

        let game_limit_reached = self.games.len() == usize::from(BEST_OF_THREE_MAX_GAMES_V1);
        let two_wins_reached = self
            .wins
            .iter()
            .any(|wins| *wins >= BEST_OF_THREE_WINS_REQUIRED_V1);
        if game_limit_reached || two_wins_reached {
            let outcome = if self.wins[0] > self.wins[1] {
                MatchOutcomeV1::Winner {
                    winner: PlayerId::P0,
                }
            } else if self.wins[1] > self.wins[0] {
                MatchOutcomeV1::Winner {
                    winner: PlayerId::P1,
                }
            } else {
                MatchOutcomeV1::Draw
            };
            self.phase = MatchPhaseV1::Complete { outcome };
            return Ok(MatchTransitionV1::Complete { outcome });
        }

        let chooser = match outcome {
            GameOutcomeV1::Win { winner } => winner.opponent(),
            GameOutcomeV1::Draw => start.player_on_draw(),
        };
        let game_index = start.game_index + 1;
        self.phase = MatchPhaseV1::AwaitingPlayDrawChoice {
            game_index,
            chooser,
        };
        Ok(MatchTransitionV1::NextGameChoice {
            game_index,
            chooser,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchStateErrorV1 {
    InvalidPlayer {
        actual: u8,
    },
    WrongChooser {
        expected: PlayerId,
        actual: PlayerId,
    },
    PlayDrawChoiceRequired,
    GameAlreadyStarted,
    MatchComplete,
}

impl fmt::Display for MatchStateErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlayer { actual } => write!(formatter, "invalid player id {actual}"),
            Self::WrongChooser { expected, actual } => write!(
                formatter,
                "player {} must make the play/draw choice, not player {}",
                expected.0, actual.0
            ),
            Self::PlayDrawChoiceRequired => {
                formatter.write_str("play/draw choice is required before recording a game result")
            }
            Self::GameAlreadyStarted => {
                formatter.write_str("the current game already has a play/draw choice")
            }
            Self::MatchComplete => formatter.write_str("the best-of-three match is complete"),
        }
    }
}

impl Error for MatchStateErrorV1 {}

fn validate_player_v1(player: PlayerId) -> Result<(), MatchStateErrorV1> {
    if player != PlayerId::P0 && player != PlayerId::P1 {
        return Err(MatchStateErrorV1::InvalidPlayer { actual: player.0 });
    }
    Ok(())
}
