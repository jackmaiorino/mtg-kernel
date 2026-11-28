//! Stand-in pod coordinator. The human and the three bot tables run together;
//! standings advance only after each table produces a natural result.

use super::play::{serve_uniform_human_game, HumanGameConfig, HumanGameResult};
use super::{
    journal, prepare_operator_config, serve_draft_build, write_json_line, DraftBuildConfig,
    LocalCardText,
};
use crate::cube_pod_runner_v1::client_swiss::{ClientSwiss, SwissResult};
use crate::cube_pod_runner_v1::{attempt_seeds, PodGameRecord, A2_GAME_CAPS};
use crate::cube_seat_package_v1::SeatPackage;
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
    serve_pod(
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
    )
}

fn serve_pod(
    package: &SeatPackage,
    config: DraftBuildConfig<'_>,
    card_text: &LocalCardText,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<(), String> {
    let mut swiss = ClientSwiss::new(config.run, config.pod_index).map_err(|e| e.to_string())?;
    let human_chair = config.human_chair;
    let draft = serve_draft_build(package, config, card_text, &mut input, &mut output)?;
    let decks = draft.decks;
    let mut private = draft.journal;
    while let Some(round) = swiss.next_round().map_err(|e| e.to_string())? {
        let human_table = round
            .tables
            .iter()
            .find(|table| table.chairs.contains(&human_chair))
            .ok_or("Swiss round is missing the human table.")?;
        let human_seat = if human_table.chairs[0] == human_chair {
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
                    human_chair,
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
                card_text,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube_draft_v1::{basic_land_ids, DECK_CARDS, SEATS};
    use crate::cube_pod_runner_v1::{pod_draft_seed, CLIENT_NAMESPACE};
    use crate::cube_seat_package_v1::tests::{load_manifest, test_directory, uniform_manifest};
    use crate::cube_seat_v1::{run_seat_draft, SeatPicker};
    use std::collections::BTreeSet;
    use std::io::Cursor;

    #[test]
    fn stand_in_pod_records_three_human_and_nine_bot_games() {
        let dir = test_directory();
        let package = load_manifest(&dir, &uniform_manifest(&dir, false)).unwrap();
        let human_chair = 3;
        let journal_path = dir.join("stand-in-pod.jsonl");
        let config = DraftBuildConfig {
            run: "fixture",
            pod_index: 1,
            human_chair,
            journal_path: &journal_path,
        };
        let draft_seed = pod_draft_seed(CLIENT_NAMESPACE, config.run, config.pod_index).unwrap();
        let mut pickers: [Box<dyn SeatPicker>; SEATS] = std::array::from_fn(|chair| {
            if chair == human_chair {
                Box::new(super::super::tests::FirstCard)
            } else {
                package.make_picker()
            }
        });
        let draft = run_seat_draft(package.pool(), draft_seed, &mut pickers)
            .unwrap()
            .record;
        let mut commands = Vec::new();
        for pick in draft.picks.iter().filter(|pick| pick.seat == human_chair) {
            write_json_line(
                &mut commands,
                &json!({
                    "kind": "pick", "round": pick.round, "pick": pick.pick, "card": pick.chosen,
                }),
            )
            .unwrap();
        }
        let cards = vec![basic_land_ids().unwrap()[0]; DECK_CARDS as usize];
        write_json_line(&mut commands, &json!({"kind": "build", "cards": cards})).unwrap();
        for _ in 0..3 {
            write_json_line(&mut commands, &json!({"kind": "concede"})).unwrap();
        }

        let mut replies = Vec::new();
        serve_pod(
            &package,
            config,
            &LocalCardText(Default::default()),
            Cursor::new(commands),
            &mut replies,
        )
        .unwrap_or_else(|e| {
            panic!(
                "stand-in pod failed: {e}. Private journal: {}",
                journal_path.display()
            )
        });
        let published: Vec<Value> = BufRead::lines(Cursor::new(replies))
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
        let events: Vec<_> = published
            .iter()
            .filter_map(|frame| frame["event"].as_str())
            .collect();
        assert_eq!(events.iter().filter(|&&event| event == "round").count(), 3);
        assert_eq!(
            events.iter().filter(|&&event| event == "standings").count(),
            3
        );
        assert_eq!(
            events
                .iter()
                .filter(|&&event| event == "game_result")
                .count(),
            3
        );
        assert!(!events.contains(&"bot_game"));
        assert_eq!(events.last(), Some(&"pod_complete"));
        let standings = published.last().unwrap()["view"]["standings"]
            .as_array()
            .unwrap();
        let human = standings
            .iter()
            .find(|standing| standing["chair"] == human_chair)
            .unwrap();
        assert_eq!(human["points"], 0);

        let private: Vec<Value> = std::fs::read_to_string(&journal_path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(private[0]["payload"]["namespace"], CLIENT_NAMESPACE);
        assert_eq!(private[0]["payload"]["use"], "client");
        assert_eq!(
            private[0]["payload"]["seat_package_sha256"],
            package.manifest_sha256()
        );
        let bots: Vec<_> = private
            .iter()
            .filter(|line| line["event"] == "bot_game")
            .collect();
        assert_eq!(bots.len(), 9);
        for game in &bots {
            assert!(!game["payload"]["scores"].is_null());
            assert!(game["payload"]["chairs"]
                .as_array()
                .unwrap()
                .iter()
                .all(|chair| chair != human_chair));
        }
        let mut games: BTreeSet<u64> = bots
            .iter()
            .map(|game| game["payload"]["game"].as_u64().unwrap())
            .collect();
        games.extend(
            private
                .iter()
                .filter(|line| line["event"] == "game_result")
                .map(|line| line["payload"]["episode"].as_u64().unwrap()),
        );
        assert!(games.iter().copied().eq(0..12));
        assert_eq!(private.last().unwrap()["event"], "pod_complete");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
