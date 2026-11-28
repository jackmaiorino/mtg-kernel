//! Stand-in pod coordinator. The human and the three bot tables run together;
//! standings advance only after each table produces a natural result.

use super::play::{serve_uniform_human_game, HumanGameConfig, HumanGameResult};
use super::{
    journal, prepare_operator_config, serve_draft_build, write_json_line, DraftBuildConfig,
};
use crate::cube_pod_runner_v1::client_swiss::{ClientSwiss, SwissResult};
use crate::cube_pod_runner_v1::{attempt_seeds, PodGameRecord, A2_GAME_CAPS};
use crate::cube_seat_v1::TypedUniformPilot;
use crate::rl::PlayerSeatV1;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Mutex;

const POD_SCHEMA: &str = "mtg-kernel-cube-client-pod-response/v1";

pub fn serve_pod_config(
    config_path: &Path,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<(), String> {
    let (config, package, card_text) = prepare_operator_config(config_path)?;
    let mut swiss = ClientSwiss::new(&config.run, config.pod_index).map_err(|e| e.to_string())?;
    let draft = serve_draft_build(
        &package,
        DraftBuildConfig {
            run: &config.run,
            pod_index: config.pod_index,
            human_chair: config.human_chair,
            journal_path: &config.journal_path,
        },
        &card_text,
        &mut input,
        &mut output,
    )?;
    let decks = draft.decks;
    let mut private = draft.journal;
    while let Some(round) = swiss.next_round().map_err(|e| e.to_string())? {
        let human_table = round
            .tables
            .iter()
            .find(|table| table.chairs.contains(&config.human_chair))
            .ok_or("Swiss round is missing the human table.")?;
        let human_seat = if human_table.chairs[0] == config.human_chair {
            PlayerSeatV1::P0
        } else {
            PlayerSeatV1::P1
        };
        journal(&mut private, "round_started", json!(round))?;
        send(
            &mut output,
            "round",
            json!({
                "round": round.round,
                "play_chair": human_table.chairs[0],
                "draw_chair": human_table.chairs[1],
                "human_seat": human_seat,
            }),
        )?;

        let (_, environment_seed, action_seed) = attempt_seeds(human_table.game_seed, 0);
        let bot_records = Mutex::new(Vec::<PodGameRecord>::new());
        let (human, bots) = std::thread::scope(|scope| {
            let games = scope.spawn(|| {
                swiss.play_bot_tables(
                    &round,
                    config.human_chair,
                    &decks,
                    &TypedUniformPilot,
                    &|game, _| {
                        bot_records
                            .lock()
                            .map_err(|_| "Bot record buffer failed.")?
                            .push(game.clone());
                        Ok(())
                    },
                )
            });
            let human = serve_uniform_human_game(
                HumanGameConfig {
                    episode: u64::from(human_table.game),
                    environment_seed,
                    action_seed,
                    human_seat,
                    caps: A2_GAME_CAPS,
                },
                human_table.chairs.map(|chair| &decks[chair]),
                &card_text,
                &mut private,
                &mut input,
                &mut output,
            );
            let bots = games
                .join()
                .map_err(|_| "The bot tables stopped unexpectedly.".to_owned())
                .and_then(|result| result.map_err(|e| e.to_string()));
            (human, bots)
        });

        let mut records = bot_records
            .into_inner()
            .map_err(|_| "Bot record buffer failed.")?;
        records.sort_unstable_by_key(|record| record.game);
        for game in records {
            journal(&mut private, "bot_game", json!(game))?;
        }
        let results = match (human, bots) {
            (Ok(human), Ok(bots)) => {
                let mut results = [SwissResult::Draw; 4];
                for (index, table) in round.tables.iter().enumerate() {
                    results[index] = if table.game == human_table.game {
                        match human {
                            HumanGameResult::Win {
                                winner: PlayerSeatV1::P0,
                            } => SwissResult::Win {
                                chair: table.chairs[0],
                            },
                            HumanGameResult::Win {
                                winner: PlayerSeatV1::P1,
                            } => SwissResult::Win {
                                chair: table.chairs[1],
                            },
                            HumanGameResult::Draw => SwissResult::Draw,
                        }
                    } else {
                        let bot = bots
                            .iter()
                            .find(|game| game.game == table.game)
                            .ok_or("A bot table result is missing.")?;
                        match bot.scores {
                            Some([1, -1]) => SwissResult::Win {
                                chair: table.chairs[0],
                            },
                            Some([-1, 1]) => SwissResult::Win {
                                chair: table.chairs[1],
                            },
                            Some([0, 0]) => SwissResult::Draw,
                            _ => return Err("Invalid bot table result.".into()),
                        }
                    }
                }
                results
            }
            (human, bots) => {
                journal(
                    &mut private,
                    "pod_stopped",
                    json!({
                        "round": round.round,
                        "human_failure": human.err(),
                        "bot_failure": bots.err(),
                    }),
                )?;
                return Err("The pod stopped before all tables finished.".into());
            }
        };
        swiss
            .finish_round(&round, results)
            .map_err(|e| e.to_string())?;
        let standings = json!({"round": round.round, "standings": swiss.standings()});
        journal(&mut private, "round_complete", standings.clone())?;
        send(&mut output, "standings", standings)?;
    }

    let final_standings = json!({"standings": swiss.standings()});
    journal(&mut private, "pod_complete", final_standings.clone())?;
    send(&mut output, "pod_complete", final_standings)
}

fn send(output: &mut impl Write, event: &str, view: Value) -> Result<(), String> {
    write_json_line(
        output,
        &json!({"schema": POD_SCHEMA, "event": event, "view": view}),
    )
    .map_err(|_| "The client response could not be written.".into())
}
