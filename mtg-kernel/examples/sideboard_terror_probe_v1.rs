//! Reproduce the old Terror mirror summary fixture using actor-visible actions.
//! Prints a bounded diagnostic, never the underlying game state or observation.

use std::collections::VecDeque;

use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};
use mtg_kernel::runtime_decks::runtime_deck_by_id;
use mtg_kernel::state::SplitMix64;
use serde_json::json;

fn main() {
    let mainboard = runtime_deck_by_id("Terror")
        .expect("Terror runtime registration")
        .card_ids
        .to_vec();
    let mut session = RlEpisodeSessionV1::reset_with_explicit_decks_and_limits(
        1,
        1,
        2000,
        200_000,
        ["Terror".to_owned(), "Terror".to_owned()],
        [mainboard.clone(), mainboard],
    )
    .expect("valid explicit Terror mirror");
    let mut rng = SplitMix64::seed(1 ^ 0x1234_5678_9abc_def0);
    let mut last_visible_actions = VecDeque::with_capacity(5);
    let mut response = session.current_response();
    loop {
        match response {
            RlSessionResponseV1::Terminal(terminal) => {
                println!(
                    "{}",
                    json!({
                        "classification": terminal.terminal_classification,
                        "code": terminal.terminal_code,
                        "reason": terminal.terminal_reason,
                        "policy_step_count": terminal.policy_step_count,
                        "physical_decision_count": terminal.physical_decision_count,
                        "last_visible_actions": last_visible_actions,
                    })
                );
                break;
            }
            RlSessionResponseV1::Decision(decision) => {
                let index = (rng.next_u64() as usize) % decision.legal_actions.len();
                let selected = &decision.legal_actions[index];
                if last_visible_actions.len() == 5 {
                    last_visible_actions.pop_front();
                }
                last_visible_actions.push_back(json!({
                    "policy_step": decision.step,
                    "physical_decision": decision.physical_decision_id,
                    "acting_player": decision.acting_player,
                    "semantic": selected.semantic,
                }));
                response = match session.step(
                    decision.episode_id,
                    decision.step,
                    index as u32,
                    &selected.stable_id,
                ) {
                    Ok(next) => next,
                    Err(error) => match session.current_response() {
                        terminal @ RlSessionResponseV1::Terminal(_) => terminal,
                        RlSessionResponseV1::Decision(_) => {
                            println!(
                                "{}",
                                json!({
                                    "classification": "session_error",
                                    "code": error.code,
                                    "reason": error.message,
                                    "policy_step_count": decision.step,
                                    "physical_decision_count": decision.physical_decision_id,
                                    "last_visible_actions": last_visible_actions,
                                })
                            );
                            std::process::exit(1);
                        }
                    },
                };
            }
        }
    }
}
