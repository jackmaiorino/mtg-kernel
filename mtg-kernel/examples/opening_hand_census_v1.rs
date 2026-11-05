//! Deal recorded games' opening hands for the mulligan noise memo.
//!
//! Reads JSON lines with `deck_ids`, `game_index`, `environment_seed`,
//! `starting_player` and `mainboards` (the per-game fields of a
//! `public-input-evaluation-match/v1` record) from stdin and writes one line
//! per input with both seats' seven-card opening hands as card ids. The deal is
//! the `keep_seven_v2` opening those games used; nothing advances gameplay.

use mtg_kernel::human_opening_v1::HumanOpeningV1;
use mtg_kernel::ids::PlayerId;
use serde::Deserialize;
use std::io::{BufRead, Write};

#[derive(Deserialize)]
struct Game {
    deck_ids: [String; 2],
    game_index: u64,
    environment_seed: u64,
    starting_player: u8,
    mainboards: [Vec<u16>; 2],
}

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for (line_number, line) in stdin.lock().lines().enumerate() {
        let line = line.expect("read stdin");
        let game: Game = serde_json::from_str(&line).expect("game record");
        let hands: Vec<Vec<u16>> = [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .map(|seat| {
                HumanOpeningV1::new(
                    game.game_index,
                    game.environment_seed,
                    1,
                    1,
                    game.deck_ids.clone(),
                    game.mainboards.clone(),
                    PlayerId(game.starting_player),
                    seat,
                )
                .expect("opening deal")
                .view()
                .hand
                .iter()
                .map(|card| card.card_id)
                .collect()
            })
            .collect();
        writeln!(
            out,
            "{}",
            serde_json::json!({ "line": line_number, "hands": hands })
        )
        .expect("write stdout");
    }
}
