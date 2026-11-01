//! Smoke and determinism checks of the export on engine games. The AC1 to
//! AC3 harnesses live in `harness`.

use super::*;
use crate::ids::PlayerId;
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::runtime_deck_by_id;
use crate::state::SplitMix64;

/// The path of the first difference between two values.
pub(crate) fn first_diff(a: &serde_json::Value, b: &serde_json::Value, path: String) -> String {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) if p == q => {}
                    (Some(p), Some(q)) => return first_diff(p, q, format!("{path}.{k}")),
                    (p, q) => return format!("{path}.{k}: {p:?} vs {q:?}"),
                }
            }
            path
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                if p != q {
                    return first_diff(p, q, format!("{path}[{i}]"));
                }
            }
            format!("{path}: lengths {} vs {}", x.len(), y.len())
        }
        _ => format!("{path}: {a} vs {b}"),
    }
}

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
                    let (x, y) = (
                        serde_json::to_value(&again).unwrap(),
                        serde_json::to_value(&a.state).unwrap(),
                    );
                    assert!(
                        x == y,
                        "the imported state serializes to the exported bytes; first difference at {}",
                        first_diff(&x, &y, String::new())
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

/// The recorded decks of a fixture state, matched by full card multisets.
fn fixture_lists(state: &crate::state::GameState) -> Vec<Vec<u16>> {
    use crate::runtime_decks::RUNTIME_DECKS;
    [PlayerId::P0, PlayerId::P1]
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
        .collect()
}

/// Ruling 41's Wildfire join (mutant L13): Lembas A waits as a pinned Food
/// source in its owner's library; an unidentified draw and a public discard
/// of a Lembas follow. Whether the discarded Lembas is A or another copy B,
/// R1b refuses in both worlds (name-level, public events only); v6's
/// identity test (L13) refuses only when it is A.
#[test]
fn wildfire_join_refuses_in_both_worlds_and_l13_differs() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;
    let record: serde_json::Value = serde_json::from_str(include_str!(
        "../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f2_s0.json"
    ))
    .unwrap();
    let base: crate::state::GameState = serde_json::from_value(record["state"].clone()).unwrap();
    let viewer = PlayerId(record["actor"].as_u64().unwrap() as u8);
    let lists = fixture_lists(&base);
    let lembas = crate::card_def::card_id_by_name("Lembas").unwrap();
    let pinned = super::export::contract_pins(&base);
    let a = *pinned
        .iter()
        .find(|id| base.objects.get(**id).card_def == lembas)
        .expect("a pinned Lembas");
    let owner = base.objects.get(a).owner;
    let Some(b) = base.players[owner.index()]
        .library
        .iter()
        .copied()
        .find(|id| *id != a && base.objects.get(*id).card_def == lembas)
    else {
        eprintln!("fixture has no second hidden Lembas; join fixture vacuous here");
        return;
    };
    let run = |which: crate::ids::ObjectId, mutant: Option<MutantV1>| {
        let mut st = base.clone();
        propose_and_commit(&mut st, ProposedEvent::zone_change(which, Zone::Hand));
        propose_and_commit(&mut st, ProposedEvent::zone_change(which, Zone::Graveyard));
        export_v1(
            &ExportInputV1 {
                state: &st,
                journal: st.observation_journal.notes(),
                viewer,
                decklists: [&lists[0], &lists[1]],
            },
            ExportOptionsV1 { mutant },
        )
        .map(|x| x.bytes)
        .map_err(|e| e.code_name().to_string())
    };
    assert_eq!(run(a, None), Err("R1b".to_string()));
    assert_eq!(run(b, None), Err("R1b".to_string()));
    let (la, lb) = (run(a, Some(MutantV1::L13)), run(b, Some(MutantV1::L13)));
    assert_ne!(
        la.is_ok(),
        lb.is_ok(),
        "L13 differs in presence: {la:?} vs {lb:?}"
    );
}

/// The public outcome a continuation reaches: life, zone sizes and public
/// zone contents by definition.
fn public_summary(s: &FastActorSessionV1) -> serde_json::Value {
    let st = s.game_state();
    let defs = |ids: &[crate::ids::ObjectId]| {
        let mut d: Vec<u16> = ids.iter().map(|id| st.objects.get(*id).card_def).collect();
        d.sort_unstable();
        d
    };
    serde_json::json!([0, 1].map(|p| {
        let pl = &st.players[p];
        serde_json::json!({"life": pl.life, "hand": pl.hand.len(), "library": pl.library.len(),
            "battlefield": defs(&pl.battlefield), "graveyard": defs(&pl.graveyard)})
    }))
}

/// Passes priority until the stack is shorter than `below` (the waiting
/// item resolved) or `limit` decisions pass; other decisions take index 0.
fn resolve_waiting(s: &mut FastActorSessionV1, below: usize, limit: usize) {
    for _ in 0..limit {
        if s.game_state().stack.len() < below {
            return;
        }
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return;
        };
        let menu = s.diagnostic_current_action_semantics().unwrap_or_default();
        let pass = menu
            .iter()
            .position(|m| matches!(m, crate::rl::ActionSemanticV1::Pass { .. }))
            .unwrap_or(0);
        s.step(d.episode_id, d.step, pass as u32).expect("step");
    }
}

/// AC3-L (design 6.5): from the recorded Lembas roots, the waiting Food
/// ability resolves to the same public outcome in the export and in the
/// true state.
#[test]
fn ac3_l_lembas_continuations_match_the_true_state() {
    for fixture in [
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f2_s0.json"),
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f3_s0.json"),
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f5_s1.json"),
    ] {
        let record: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let state: crate::state::GameState =
            serde_json::from_value(record["state"].clone()).unwrap();
        let viewer = PlayerId(record["actor"].as_u64().unwrap() as u8);
        let lists = fixture_lists(&state);
        let x = export_v1(
            &ExportInputV1 {
                state: &state,
                journal: &[],
                viewer,
                decklists: [&lists[0], &lists[1]],
            },
            ExportOptionsV1::default(),
        )
        .unwrap_or_else(|e| panic!("{}: {e:?}", record["case"]));
        let live = FastActorSessionV1::from_v3_fixture_state(state.clone());
        let below = state.stack.len();
        let mut truth = live.clone();
        let mut canon = FastActorSessionV1::import_infoset_v1(&live, x.state.clone(), 0);
        resolve_waiting(&mut truth, below, 40);
        resolve_waiting(&mut canon, below, 40);
        let (t, c) = (public_summary(&truth), public_summary(&canon));
        assert!(
            t == c,
            "{}: continuations differ at {}; stack {:?}; responses {:?} / {:?}",
            record["case"],
            first_diff(&t, &c, String::new()),
            state
                .stack
                .iter()
                .map(|i| (i.controller, state.objects.get(i.source).card_def))
                .collect::<Vec<_>>(),
            truth.current_response(),
            canon.current_response()
        );
    }
}
