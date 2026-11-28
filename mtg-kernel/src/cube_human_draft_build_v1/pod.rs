//! Stand-in pod coordinator. The human and the three bot tables run together;
//! standings advance only after each table produces a natural result.
//! A failed human game stops the pod without a replay.

use super::play::{serve_uniform_human_game, HumanGameConfig, HumanGameResult};
use super::{
    journal, prepare_operator_config, serve_draft_build, write_json_line, DraftBuildConfig,
    LocalCardText,
};
use crate::cube_pod_runner_v1::client_swiss::{ClientSwiss, SwissResult};
use crate::cube_pod_runner_v1::{attempt_seeds, A2_GAME_CAPS};
use crate::cube_seat_package_v1::SeatPackage;
use crate::cube_seat_v1::TypedUniformPilot;
use crate::rl::PlayerSeatV1;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;

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
    let human_chair = config.human_chair;
    let mut swiss =
        ClientSwiss::new(config.run, config.pod_index, human_chair).map_err(|e| e.to_string())?;
    let draft = serve_draft_build(package, config, card_text, &mut input, &mut output)?;
    let decks = draft.decks;
    let private = draft.journal;
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
        journal(&private, "round_started", json!(round))?;
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
        let (human, bots) = std::thread::scope(|scope| {
            let games = scope.spawn(|| {
                swiss.play_bot_tables(&round, &decks, &TypedUniformPilot, &|game, _| {
                    journal(&private, "bot_game", json!(game))
                })
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
                &private,
                &mut input,
                &mut output,
            );
            let bots = games
                .join()
                .map_err(|_| "The bot tables stopped unexpectedly.".to_owned())
                .and_then(|result| result.map_err(|e| e.to_string()));
            (human, bots)
        });

        let human_result = match (human, bots) {
            (Ok(human), Ok(_)) => match human {
                HumanGameResult::Win {
                    winner: PlayerSeatV1::P0,
                } => SwissResult::Win {
                    chair: human_table.chairs[0],
                },
                HumanGameResult::Win {
                    winner: PlayerSeatV1::P1,
                } => SwissResult::Win {
                    chair: human_table.chairs[1],
                },
                HumanGameResult::Draw => SwissResult::Draw,
            },
            (human, bots) => {
                journal(
                    &private,
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
            .finish_round(&round, human_result)
            .map_err(|e| e.to_string())?;
        let standings = json!({"round": round.round, "standings": swiss.standings()});
        journal(&private, "round_complete", standings.clone())?;
        send(&mut output, "standings", standings)?;
    }

    let final_standings = json!({"standings": swiss.standings()});
    journal(&private, "pod_complete", final_standings.clone())?;
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
    use std::cell::RefCell;
    use std::collections::{BTreeSet, VecDeque};
    use std::io::{BufReader, Cursor, Read};
    use std::rc::Rc;

    struct ScriptedInput {
        draft_and_build: Cursor<Vec<u8>>,
        opening_commands: Rc<RefCell<VecDeque<u8>>>,
    }

    impl Read for ScriptedInput {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            let read = self.draft_and_build.read(output)?;
            if read != 0 {
                return Ok(read);
            }
            let mut commands = self.opening_commands.borrow_mut();
            let count = output.len().min(commands.len());
            for byte in &mut output[..count] {
                *byte = commands.pop_front().unwrap();
            }
            Ok(count)
        }
    }

    struct ScriptedOutput<'a> {
        line: Vec<u8>,
        frames: &'a mut Vec<Value>,
        opening_commands: Rc<RefCell<VecDeque<u8>>>,
        last_concession: Option<Value>,
    }

    impl Write for ScriptedOutput<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.line.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.line.is_empty() {
                return Ok(());
            }
            let frame: Value = serde_json::from_slice(&self.line)?;
            self.line.clear();
            if frame["event"] == "opening" {
                let command = json!({
                    "kind": "concede",
                    "episode": frame["view"]["episode"],
                    "prompt_seq": frame["view"]["prompt_seq"],
                });
                let mut requests = Vec::new();
                if let Some(prior) = self.last_concession.replace(command.clone()) {
                    write_json_line(&mut requests, &prior)?;
                }
                write_json_line(&mut requests, &command)?;
                self.opening_commands.borrow_mut().extend(requests);
            }
            self.frames.push(frame);
            Ok(())
        }
    }

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
        let opening_commands = Rc::new(RefCell::new(VecDeque::new()));
        let input = BufReader::new(ScriptedInput {
            draft_and_build: Cursor::new(commands),
            opening_commands: Rc::clone(&opening_commands),
        });
        let mut published = Vec::new();
        let output = ScriptedOutput {
            line: Vec::new(),
            frames: &mut published,
            opening_commands,
            last_concession: None,
        };
        serve_pod(
            &package,
            config,
            &LocalCardText(Default::default()),
            input,
            output,
        )
        .unwrap_or_else(|e| {
            panic!(
                "stand-in pod failed: {e}. Private journal: {}",
                journal_path.display()
            )
        });
        let events: Vec<_> = published
            .iter()
            .map(|frame| frame["event"].as_str().unwrap())
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
        assert!(events.iter().all(|event| matches!(
            *event,
            "start"
                | "pick"
                | "build"
                | "build_accepted"
                | "round"
                | "opening"
                | "game_result"
                | "standings"
                | "error"
                | "pod_complete"
        )));
        assert_eq!(events.last(), Some(&"pod_complete"));
        assert_eq!(
            events.iter().filter(|&&event| event == "opening").count(),
            3
        );
        assert_eq!(events.iter().filter(|&&event| event == "error").count(), 2);
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
        let mut points = [0u8; SEATS];
        let schedule = private
            .iter()
            .filter(|line| line["event"] == "round_started");
        let public_standings = published.iter().filter(|line| line["event"] == "standings");
        let public_rounds = published.iter().filter(|line| line["event"] == "round");
        let mut rounds = 0;
        let mut last_standings = Vec::new();
        for ((round, public_round), standings) in schedule.zip(public_rounds).zip(public_standings)
        {
            let round = &round["payload"];
            assert_eq!(round["round"], rounds);
            rounds += 1;
            let tables = round["tables"].as_array().unwrap();
            assert_eq!(tables.len(), 4);
            for table in tables {
                let id = &table["game"];
                let chairs = table["chairs"].as_array().unwrap();
                let chairs = [
                    chairs[0].as_u64().unwrap() as usize,
                    chairs[1].as_u64().unwrap() as usize,
                ];
                if chairs.contains(&human_chair) {
                    assert_eq!(
                        public_round["view"],
                        json!({
                            "round": round["round"],
                            "play_chair": chairs[0],
                            "draw_chair": chairs[1],
                            "human_seat": if chairs[0] == human_chair { "p0" } else { "p1" },
                        }),
                    );
                    let record = private
                        .iter()
                        .find(|line| {
                            line["event"] == "game_result" && line["payload"]["episode"] == *id
                        })
                        .unwrap();
                    assert_eq!(record["payload"]["reason"], "conceded");
                    let opponent = usize::from(chairs[0] == human_chair);
                    assert_eq!(
                        record["payload"]["result"]["Win"]["winner"],
                        if opponent == 0 { "p0" } else { "p1" }
                    );
                    points[chairs[opponent]] += 3;
                } else {
                    let record = bots
                        .iter()
                        .find(|line| line["payload"]["game"] == *id)
                        .unwrap();
                    let record = &record["payload"];
                    assert_eq!(record["chairs"], json!(chairs));
                    let terminal = &record["attempts"].as_array().unwrap().last().unwrap()["end"];
                    assert_eq!(terminal["kind"], "natural");
                    if let Some(winner) = terminal["winner_seat"].as_u64() {
                        let winner = winner as usize;
                        assert!(chairs.contains(&winner));
                        points[winner] += 3;
                    } else {
                        for chair in chairs {
                            points[chair] += 1;
                        }
                    }
                }
            }
            let mut chairs: Vec<_> = (0..SEATS).collect();
            chairs.sort_unstable_by_key(|&chair| (std::cmp::Reverse(points[chair]), chair));
            let expected: Vec<_> = chairs
                .into_iter()
                .map(|chair| {
                    json!({
                        "chair": chair, "points": points[chair],
                    })
                })
                .collect();
            assert_eq!(
                standings["view"],
                json!({"round": round["round"], "standings": expected})
            );
            last_standings = expected;
        }
        assert_eq!(rounds, 3);
        assert_eq!(
            published.last().unwrap()["view"],
            json!({"standings": last_standings})
        );
        assert_eq!(private.last().unwrap()["event"], "pod_complete");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
