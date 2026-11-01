//! Smoke and determinism checks of the export on engine games. The AC1 to
//! AC3 harnesses live in `harness`.

use super::*;
use crate::ids::PlayerId;
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::runtime_deck_by_id;
use crate::state::SplitMix64;

pub(crate) fn decklists(s: &FastActorSessionV1) -> [Vec<u16>; 2] {
    let ids = s.deck_ids_v1();
    [0, 1].map(|i| {
        runtime_deck_by_id(&ids[i])
            .expect("registered deck")
            .card_ids
            .to_vec()
    })
}

pub(crate) fn export_at(
    s: &FastActorSessionV1,
    viewer: PlayerId,
    options: ExportOptionsV1,
) -> Result<InfosetV1, ExportErrorV1> {
    let lists = decklists(s);
    let state = s.game_state();
    export_v1(
        &ExportInputV1 {
            state,
            journal: state.observation_journal.notes(),
            viewer,
            decklists: [&lists[0], &lists[1]],
        },
        options,
    )
}

fn seat(p: PlayerSeatV1) -> PlayerId {
    match p {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

/// Plays a seeded uniform-random game and calls `at` at every decision with
/// at least two candidates.
pub(crate) fn play(
    decks: [&str; 2],
    seed: u64,
    max_decisions: usize,
    mut at: impl FnMut(&FastActorSessionV1, PlayerId),
) {
    let mut s = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        seed,
        seed,
        2_000,
        20_000,
        [decks[0].to_string(), decks[1].to_string()],
    )
    .expect("reset");
    let mut rng = SplitMix64::seed(seed ^ 0x9e37);
    for _ in 0..max_decisions {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return;
        };
        if d.legal_action_count >= 2 {
            at(&s, seat(d.acting_player));
        }
        let a = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
        s.step(d.episode_id, d.step, a).expect("legal step");
    }
}

#[test]
fn export_is_deterministic_and_round_trips_through_import() {
    let mut tally = RefusalTallyV1::default();
    for (decks, seed) in [(["CawGates", "Faeries"], 3u64), (["Wildfire", "Wildfire"], 5)] {
        play(decks, seed, 400, |s, v| {
            let a = export_at(s, v, ExportOptionsV1::default());
            let b = export_at(s, v, ExportOptionsV1::default());
            match (&a, &b) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(a.bytes, b.bytes, "export is a function of its input");
                    let imported = FastActorSessionV1::import_infoset_v1(s, a.state.clone(), 0);
                    let again: crate::state::GameState =
                        serde_json::from_slice(&serde_json::to_vec(imported.game_state()).unwrap())
                            .unwrap();
                    assert_eq!(
                        serde_json::to_vec(&again).unwrap(),
                        serde_json::to_vec(&a.state).unwrap(),
                        "export, import, export is byte-equal"
                    );
                }
                (Err(x), Err(y)) => assert_eq!(x, y),
                _ => panic!("presence differs between two exports of one state"),
            }
            tally.record(&a.as_ref().map(|_| ()).map_err(Clone::clone));
        });
    }
    eprintln!("infoset smoke tally: {}", serde_json::to_string(&tally).unwrap());
    assert!(tally.exported > 0, "some decisions export");
}
