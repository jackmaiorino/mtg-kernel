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
fn export_is_deterministic_and_imports_byte_for_byte() {
    let mut tally = RefusalTallyV1::default();
    for (decks, seed) in [
        (["CawGates", "Faeries"], 3u64),
        (["Wildfire", "Wildfire"], 5),
    ] {
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
                        "the imported state serializes to the exported bytes"
                    );
                }
                (Err(x), Err(y)) => assert_eq!(x, y),
                _ => panic!("presence differs between two exports of one state"),
            }
            if let Err(e) = &a {
                let seen = tally.by_code.get(e.code_name()).copied().unwrap_or(0);
                if seen < 3 {
                    eprintln!("refusal {}: {e:?}", e.code_name());
                }
            }
            tally.record(&a.as_ref().map(|_| ()).map_err(Clone::clone));
        });
    }
    eprintln!(
        "infoset smoke tally: {}",
        serde_json::to_string(&tally).unwrap()
    );
    assert!(tally.exported > 0, "some decisions export");
}

/// Astra r1 P1: an unidentified put-back never names the card, even when V
/// had it revealed: two worlds that return a revealed card and an unknown
/// one in either order must give the same tombstones and the same export.
#[test]
fn unidentified_put_back_of_a_revealed_card_is_never_named() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;
    let live = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        41,
        41,
        2_000,
        20_000,
        ["CawGates".to_string(), "Faeries".to_string()],
    )
    .expect("reset");
    let viewer = PlayerId::P0;
    let opp = viewer.opponent();
    let base = live.game_state().clone();
    let hand = base.players[opp.index()].hand.clone();
    assert!(hand.len() >= 2);
    let worlds: Vec<crate::state::GameState> = [(hand[0], hand[1]), (hand[1], hand[0])]
        .into_iter()
        .map(|(first, second)| {
            let mut st = base.clone();
            // V sees hand[0] revealed in both worlds; the opponent then
            // returns two cards unidentified, in either order.
            st.reveal_hand_card(viewer, opp, hand[0]).expect("reveal");
            propose_and_commit(&mut st, ProposedEvent::zone_change(first, Zone::Library));
            propose_and_commit(&mut st, ProposedEvent::zone_change(second, Zone::Library));
            st
        })
        .collect();
    let projections: Vec<_> = worlds
        .iter()
        .map(|st| super::projection::project_v1(st, st.observation_journal.notes(), viewer))
        .collect();
    let n = worlds[0].engine.event_history.len();
    for e in [n - 2, n - 1] {
        for p in &projections {
            assert!(
                p.event_observed[e].is_empty(),
                "an unidentified put-back is observed at event {e}"
            );
        }
    }
}

/// W-pin-1 shape on recorded S1 roots (Wildfire, Lembas shuffled into its
/// owner's library while its Food ability waits): the pin is placed as a
/// `U:` occupant with Lembas's definition on the canonical epoch path, and
/// the waiting contract maps to epoch 1 (design 6.5 case B or C).
#[test]
fn lembas_waiting_source_is_pinned_on_the_canonical_epoch_path() {
    use crate::runtime_decks::RUNTIME_DECKS;
    for fixture in [
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f2_s0.json"),
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f3_s0.json"),
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f5_s1.json"),
    ] {
        let record: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let state: crate::state::GameState =
            serde_json::from_value(record["state"].clone()).unwrap();
        let viewer = PlayerId(record["actor"].as_u64().unwrap() as u8);
        // The recorded games' decks, matched by their full card multisets.
        let lists: Vec<Vec<u16>> = [PlayerId::P0, PlayerId::P1]
            .iter()
            .map(|owner| {
                let mut cards: Vec<u16> = state
                    .objects
                    .iter()
                    .filter(|(_, o)| {
                        o.owner == *owner && !o.v4.is_token && o.spell_copy_origin.is_none()
                    })
                    .map(|(_, o)| o.card_def)
                    .collect();
                cards.sort_unstable();
                RUNTIME_DECKS
                    .iter()
                    .map(|d| {
                        let mut ids = d.card_ids.to_vec();
                        ids.sort_unstable();
                        ids
                    })
                    .find(|ids| *ids == cards)
                    .expect("a registered deck")
            })
            .collect();
        let export = export_v1(
            &ExportInputV1 {
                state: &state,
                journal: &[],
                viewer,
                decklists: [&lists[0], &lists[1]],
            },
            ExportOptionsV1::default(),
        );
        let x = match export {
            Ok(x) => x,
            Err(e) => panic!("{}: {e:?}", record["case"]),
        };
        let lembas = crate::card_def::card_id_by_name("Lembas").unwrap();
        let pinned: Vec<(usize, &crate::state::GameObject)> = x
            .state
            .objects
            .iter()
            .filter(|(id, _)| matches!(x.handles[id.0 as usize], HandleV1::U { pinned: true, .. }))
            .map(|(id, o)| (id.0 as usize, o))
            .collect();
        assert_eq!(pinned.len(), 1, "{}", record["case"]);
        let (pin_id, pin) = pinned[0];
        assert_eq!(pin.card_def, lembas);
        let item = x
            .state
            .stack
            .iter()
            .find(|i| i.source.0 as usize == pin_id)
            .expect("the waiting ability names the pinned occupant");
        let contract = item.v4.ability_source_contract.expect("contract");
        assert_eq!(contract.zone_change_count, 1, "the contract epoch is p0");
        assert!(
            [3, 4].contains(&pin.zone_change_count),
            "{}: pinned generation {} is phi(e_last) + h",
            record["case"],
            pin.zone_change_count
        );
    }
}
