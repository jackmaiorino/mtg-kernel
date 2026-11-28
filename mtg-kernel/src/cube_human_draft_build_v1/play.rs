//! Interactive half of a client Swiss table, using the human match's London
//! opening and fixed-seat projector. Stand-ins keep seven and play uniformly.

use super::{journal, DraftTransport, LocalCardText};
use crate::cube_draft_v1::{uniform_index, Deck, GameCaps};
use crate::human_bo3_v1::{HumanActionRequestV1, HumanDecisionProjectorV1};
use crate::human_opening_v1::{HumanOpeningPhaseV1, HumanOpeningV1};
use crate::ids::PlayerId;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::FastActorResponseV1;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::File;
use std::io::{BufRead, Write};

const PLAY_SCHEMA: &str = "mtg-kernel-cube-client-play-response/v1";

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Keep {},
    Mulligan {},
    Bottom {
        order: Vec<u32>,
    },
    Action {
        episode: u64,
        prompt_seq: u64,
        action_index: u32,
    },
    Concede {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HumanGameResult {
    Win { winner: PlayerSeatV1 },
    Draw,
}

pub struct HumanGameConfig {
    pub episode: u64,
    pub environment_seed: u64,
    pub action_seed: u64,
    pub human_seat: PlayerSeatV1,
    pub caps: GameCaps,
}

pub fn serve_uniform_human_game(
    config: HumanGameConfig,
    decks: [&Deck; 2],
    card_text: &LocalCardText,
    private: &mut File,
    input: impl BufRead,
    output: impl Write,
) -> Result<HumanGameResult, String> {
    let human = match config.human_seat {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    };
    let mut opening = HumanOpeningV1::new_cube_limited(
        config.episode,
        config.environment_seed,
        config.caps,
        decks.map(Deck::card_ids),
        human,
    )?;
    journal(
        private,
        "game_started",
        json!({
            "episode": config.episode,
            "human_seat": config.human_seat,
            "environment_seed": config.environment_seed,
            "action_seed": config.action_seed,
            "policy": "Uniform stand-in",
        }),
    )?;

    let mut transport = DraftTransport {
        schema: PLAY_SCHEMA,
        input,
        output,
        line: String::new(),
    };
    let conceded = HumanGameResult::Win {
        winner: human.opponent().into(),
    };
    while opening.view().phase != HumanOpeningPhaseV1::Ready {
        let view = opening.view();
        let ids: Vec<_> = view.hand.iter().map(|card| card.card_id).collect();
        transport.send(
            "opening",
            json!({
                "opening": view,
                "card_text": card_text.render(&ids),
            }),
        )?;
        loop {
            let (accepted, payload) = match transport.command()? {
                Some(Command::Keep {}) => (opening.keep(), json!({"kind": "keep"})),
                Some(Command::Mulligan {}) => (opening.mulligan(), json!({"kind": "mulligan"})),
                Some(Command::Bottom { order }) => (
                    opening.bottom(&order),
                    json!({"kind": "bottom", "order": order}),
                ),
                Some(Command::Concede {}) => {
                    return finish(
                        &mut transport,
                        private,
                        config.episode,
                        conceded,
                        "conceded",
                    );
                }
                Some(Command::Action { .. }) => (
                    Err("Complete the opening before playing an action.".into()),
                    json!(null),
                ),
                None => continue,
            };
            if let Err(message) = accepted {
                transport.error(&message)?;
                continue;
            }
            journal(
                private,
                "human_opening",
                json!({
                    "episode": config.episode,
                    "choice": payload,
                }),
            )?;
            break;
        }
    }

    let mut game = opening.into_session()?;
    let mut projector = HumanDecisionProjectorV1::new(config.human_seat);
    let mut rng = SplitMix64::seed(config.action_seed);
    let mut decisions = 0u64;
    loop {
        match game.current_response() {
            FastActorResponseV1::Terminal(terminal)
                if terminal.terminal_classification == TerminalClassificationV1::Natural =>
            {
                let result =
                    terminal
                        .winner
                        .map_or(HumanGameResult::Draw, |winner| HumanGameResult::Win {
                            winner,
                        });
                return finish(&mut transport, private, config.episode, result, "natural");
            }
            FastActorResponseV1::Terminal(terminal) => {
                journal(
                    private,
                    "game_stopped",
                    json!({
                        "episode": config.episode,
                        "terminal": terminal,
                    }),
                )?;
                return Err("The engine stopped this game without a result.".into());
            }
            FastActorResponseV1::Decision(decision) => {
                if decisions >= config.caps.max_decisions {
                    journal(
                        private,
                        "game_stopped",
                        json!({
                            "episode": config.episode, "reason": "driver decision cap",
                        }),
                    )?;
                    return Err("The game reached its decision limit without a result.".into());
                }
                if decision.acting_player != config.human_seat {
                    if decision.legal_action_count == 0 {
                        return Err("The stand-in has no legal action.".into());
                    }
                    let selected =
                        uniform_index(&mut rng, decision.legal_action_count as usize) as u32;
                    journal(
                        private,
                        "bot_action",
                        json!({
                            "episode": config.episode, "step": decision.step, "selected": selected,
                        }),
                    )?;
                    game.step(decision.episode_id, decision.step, selected)
                        .map_err(|_| "The stand-in action could not be applied.")?;
                } else {
                    let visible = projector
                        .project_current(&game, decision)
                        .map_err(|error| error.to_string())?;
                    let expected_prompt = visible.prompt_seq;
                    transport.send(
                        "decision",
                        json!({"episode": config.episode, "decision": visible}),
                    )?;
                    loop {
                        match transport.command()? {
                            Some(Command::Action {
                                episode,
                                prompt_seq,
                                action_index,
                            }) if episode == config.episode && prompt_seq == expected_prompt => {
                                let request = HumanActionRequestV1 {
                                    prompt_seq,
                                    action_index,
                                };
                                match projector.submit(&mut game, request) {
                                    Ok(receipt) => {
                                        journal(
                                            private,
                                            "human_action",
                                            json!({
                                                "episode": config.episode, "receipt": receipt,
                                            }),
                                        )?;
                                        break;
                                    }
                                    Err(error) => transport.error(&error.to_string())?,
                                }
                            }
                            Some(Command::Concede {}) => {
                                return finish(
                                    &mut transport,
                                    private,
                                    config.episode,
                                    conceded,
                                    "conceded",
                                );
                            }
                            Some(_) => {
                                transport.error("Choose an action from the current prompt.")?
                            }
                            None => {}
                        }
                    }
                }
                decisions += 1;
            }
        }
    }
}

fn finish(
    transport: &mut DraftTransport<impl BufRead, impl Write>,
    private: &mut File,
    episode: u64,
    result: HumanGameResult,
    reason: &'static str,
) -> Result<HumanGameResult, String> {
    let view = json!({"episode": episode, "result": result, "reason": reason});
    journal(private, "game_result", view.clone())?;
    transport.send("game_result", view)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::cube_draft_v1::DECK_CARDS;
    use crate::cube_pod_runner_v1::A2_GAME_CAPS;
    use crate::cube_seat_package_v1::tests::test_directory;
    use crate::cube_seat_v1::canonical_deck;
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::io::Cursor;

    #[test]
    fn limited_london_and_fixed_view_keep_the_bot_hand_private() {
        let dir = test_directory();
        let island = card_id_by_name("Island").unwrap();
        let mountain = card_id_by_name("Mountain").unwrap();
        let human_deck = canonical_deck(&[island; DECK_CARDS as usize]);
        let bot_deck = canonical_deck(&[mountain; DECK_CARDS as usize]);
        let card_text = LocalCardText(BTreeMap::from([
            (island, "own card text".into()),
            (mountain, "opponent card text".into()),
        ]));
        let input = concat!(
            "{\"kind\":\"mulligan\"}\n",
            "{\"kind\":\"keep\"}\n",
            "{\"kind\":\"bottom\",\"order\":[0]}\n",
            "{\"kind\":\"action\",\"episode\":0,\"prompt_seq\":1,\"action_index\":0}\n",
            "{\"kind\":\"action\",\"episode\":1,\"prompt_seq\":1,\"action_index\":0}\n",
            "{\"kind\":\"action\",\"episode\":1,\"prompt_seq\":1,\"action_index\":0}\n",
            "{\"kind\":\"concede\"}\n",
        );
        let mut journal = File::create(dir.join("private.jsonl")).unwrap();
        let mut output = Vec::new();
        let result = serve_uniform_human_game(
            HumanGameConfig {
                episode: 1,
                environment_seed: 2,
                action_seed: 3,
                human_seat: PlayerSeatV1::P0,
                caps: A2_GAME_CAPS,
            },
            [&human_deck, &bot_deck],
            &card_text,
            &mut journal,
            Cursor::new(input),
            &mut output,
        )
        .unwrap();
        assert_eq!(
            result,
            HumanGameResult::Win {
                winner: PlayerSeatV1::P1
            }
        );
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("own card text"));
        assert!(!output.contains("opponent card text"));
        let frames: Vec<Value> = output
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(frames
            .iter()
            .filter(|frame| frame["event"] == "opening")
            .all(|frame| !frame.to_string().contains("Mountain")));
        assert_eq!(frames[2]["view"]["opening"]["required_bottom"], 1);
        assert_eq!(frames[2]["view"]["opening"]["opponent_hand_count"], 7);
        let first = frames
            .iter()
            .find(|frame| frame["event"] == "decision")
            .unwrap();
        assert_eq!(first["view"]["episode"], 1);
        assert!(!first.to_string().contains("Mountain"));
        assert_eq!(
            first["view"]["decision"]["state"]["own_hand"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
        assert_eq!(
            frames
                .iter()
                .filter(|frame| frame["event"] == "error")
                .count(),
            2
        );
        assert_eq!(frames.last().unwrap()["view"]["reason"], "conceded");
        drop(journal);
        let private = std::fs::read_to_string(dir.join("private.jsonl")).unwrap();
        assert_eq!(
            private
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .filter(|frame| frame["event"] == "human_action")
                .count(),
            1,
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
