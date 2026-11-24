//! Per-seat priority-window rules on the trainer's sessions
//! (`priority_action_windows_v1`, gate g2 of the S0 design).
//!
//! A seat on `ActionWindowsV1` that answers Pass at every window its rule adds
//! plays today's game: the same game state at every decision both games
//! share, the same menus, the same terminal. Mana-only windows are passed in
//! both games (today's surface asks them, the rule asks some of them); every
//! other decision takes the same seeded answer in both games. Both resets
//! (plain and London) are covered, every deck plays, both seats take the
//! rule, and the rule must add at least one window over the deals.

use super::*;
use crate::priority_action_windows_v1::SeatPriorityWindowsV1;
use crate::rl::ActionSemanticV1;
use crate::runtime_decks::runtime_deck_by_id;
use crate::state::SplitMix64;
use PauperMulliganModeV1::Learned;

const DECKS: [&str; 9] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries",
];
const LIMITS: (u64, u64) = (200_000, 2_000_000);
const MAX_SHARED: u64 = 700;

fn session(
    london: bool,
    decks: [&str; 2],
    seed: u64,
    starting: PlayerId,
    windows: [SeatPriorityWindowsV1; 2],
) -> FastActorSessionV1 {
    let card_ids = decks.map(|id| runtime_deck_by_id(id).unwrap().card_ids.to_vec());
    let labels = decks.map(str::to_owned);
    if london {
        FastActorSessionV1::reset_pauper_london_windows_v1(
            3,
            seed,
            LIMITS.0,
            LIMITS.1,
            labels,
            card_ids,
            starting,
            [Learned, Learned],
            windows,
        )
        .unwrap()
    } else {
        FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_and_windows_v1(
            3, seed, LIMITS.0, LIMITS.1, labels, card_ids, starting, windows,
        )
        .unwrap()
    }
}

fn candidates(session: &FastActorSessionV1) -> &[crate::rl::CorePolicyActionCandidateV1] {
    &session
        .current
        .as_ref()
        .expect("a pending decision")
        .candidates
}

fn pass_index(session: &FastActorSessionV1) -> Option<u32> {
    candidates(session)
        .iter()
        .position(|c| matches!(c.semantic, ActionSemanticV1::Pass { .. }))
        .map(|i| i as u32)
}

/// A priority window whose only options besides Pass are mana abilities.
fn mana_only(session: &FastActorSessionV1) -> bool {
    pass_index(session).is_some()
        && candidates(session).iter().all(|c| {
            matches!(
                c.semantic,
                ActionSemanticV1::Pass { .. } | ActionSemanticV1::ActivateManaAbility { .. }
            )
        })
}

fn pass(session: &mut FastActorSessionV1, decision: &FastActorDecisionV1) {
    let index = pass_index(session).expect("a priority window offers Pass");
    session
        .step(decision.episode_id, decision.step, index)
        .unwrap();
}

/// Advances `session` past every decision only one of the two games has,
/// answering each with Pass. Returns the next shared decision, or None at
/// the terminal.
fn next_shared(session: &mut FastActorSessionV1, added: &mut u64) -> Option<FastActorDecisionV1> {
    loop {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            return None;
        };
        if session.surface.engine_window_v1().is_some() {
            *added += 1;
            pass(session, &decision);
            continue;
        }
        if mana_only(session) {
            pass(session, &decision);
            continue;
        }
        return Some(decision);
    }
}

#[derive(Default)]
struct Outcome {
    shared: u64,
    added: u64,
    finished: bool,
}

fn pass_through(
    london: bool,
    decks: [&str; 2],
    seed: u64,
    starting: PlayerId,
    seat: usize,
) -> Outcome {
    let mut windows = [SeatPriorityWindowsV1::HarnessV2; 2];
    windows[seat] = SeatPriorityWindowsV1::ActionWindowsV1;
    let mut plain = session(
        london,
        decks,
        seed,
        starting,
        [SeatPriorityWindowsV1::HarnessV2; 2],
    );
    let mut ruled = session(london, decks, seed, starting, windows);
    let mut rng = SplitMix64::seed(seed ^ 0xA11D);
    let mut out = Outcome::default();
    let mut plain_added = 0;
    let context = |shared: u64| {
        format!("{decks:?} seed {seed} seat {seat} london {london}: shared decision {shared}")
    };
    loop {
        let theirs = next_shared(&mut plain, &mut plain_added);
        let ours = next_shared(&mut ruled, &mut out.added);
        assert_eq!(plain_added, 0, "{}", context(out.shared));
        assert_eq!(
            plain.game_state(),
            ruled.game_state(),
            "{}",
            context(out.shared)
        );
        let (Some(theirs), Some(ours)) = (theirs, ours) else {
            // The same terminal but for the two session counters: the ruled
            // game counts the added windows it passed and not the mana-only
            // windows its rule passed for it.
            let counters_cleared = |response: FastActorResponseV1| match response {
                FastActorResponseV1::Terminal(mut terminal) => {
                    terminal.policy_step_count = 0;
                    terminal.physical_decision_count = 0;
                    FastActorResponseV1::Terminal(terminal)
                }
                decision => decision,
            };
            assert_eq!(
                counters_cleared(plain.current_response()),
                counters_cleared(ruled.current_response()),
                "{}",
                context(out.shared)
            );
            out.finished = true;
            return out;
        };
        assert_eq!(
            theirs.acting_player,
            ours.acting_player,
            "{}",
            context(out.shared)
        );
        assert_eq!(
            theirs.decision_kind,
            ours.decision_kind,
            "{}",
            context(out.shared)
        );
        let menu: Vec<_> = candidates(&plain)
            .iter()
            .map(|c| c.semantic.clone())
            .collect();
        let menu_ruled: Vec<_> = candidates(&ruled)
            .iter()
            .map(|c| c.semantic.clone())
            .collect();
        assert_eq!(menu, menu_ruled, "{}", context(out.shared));
        if out.shared >= MAX_SHARED {
            return out;
        }
        let pick = (rng.next_u64() % menu.len() as u64) as u32;
        plain.step(theirs.episode_id, theirs.step, pick).unwrap();
        ruled.step(ours.episode_id, ours.step, pick).unwrap();
        out.shared += 1;
    }
}

#[test]
fn an_action_window_seat_that_passes_every_added_window_plays_todays_game() {
    let mut added = 0;
    let mut finished = 0;
    let mut deals = 0;
    for (i, deck) in DECKS.iter().enumerate() {
        for (j, london) in [false, true].into_iter().enumerate() {
            let other = DECKS[(i + 4 + j) % DECKS.len()];
            for seat in 0..2 {
                let seed = 0x5eed_0a00 + (i * 4 + j * 2 + seat) as u64;
                let starting = PlayerId(((i + seat) % 2) as u8);
                let out = pass_through(london, [deck, other], seed, starting, seat);
                added += out.added;
                finished += u64::from(out.finished);
                deals += 1;
            }
        }
    }
    assert_eq!(deals, 36);
    assert!(added > 0, "the rule added no window over {deals} deals");
    assert!(finished > 0, "no deal reached its terminal");
}

#[test]
fn both_seats_harness_v2_is_the_plain_reset() {
    for (i, deck) in DECKS.iter().enumerate() {
        let decks = [*deck, DECKS[(i + 2) % DECKS.len()]];
        for london in [false, true] {
            let seed = 0x5eed_0b00 + i as u64;
            let starting = PlayerId((i % 2) as u8);
            let card_ids = decks.map(|id| runtime_deck_by_id(id).unwrap().card_ids.to_vec());
            let labels = decks.map(str::to_owned);
            let plain = if london {
                FastActorSessionV1::reset_pauper_london_v1(
                    3,
                    seed,
                    LIMITS.0,
                    LIMITS.1,
                    labels,
                    card_ids,
                    starting,
                    [Learned, Learned],
                )
                .unwrap()
            } else {
                FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
                    3, seed, LIMITS.0, LIMITS.1, labels, card_ids, starting,
                )
                .unwrap()
            };
            let ruled = session(
                london,
                decks,
                seed,
                starting,
                [SeatPriorityWindowsV1::HarnessV2; 2],
            );
            assert_eq!(plain.game_state(), ruled.game_state());
            assert_eq!(plain.current_response(), ruled.current_response());
        }
    }
}

/// A session rebuilt around another session's state (the W1 information-set
/// import) keeps that session's rules: they are session configuration, like
/// its contract mode, and the import's fresh surface would otherwise pass
/// the windows a rule hands its seat. A session without rules imports as
/// before.
#[test]
fn an_infoset_import_keeps_the_templates_rules() {
    let rules = [
        SeatPriorityWindowsV1::ActionWindowsV1,
        SeatPriorityWindowsV1::ActionWindowsEndStepV1,
    ];
    for windows in [rules, [SeatPriorityWindowsV1::HarnessV2; 2]] {
        let live = session(
            false,
            ["Burn", "Faeries"],
            0x5eed_0d00,
            PlayerId::P0,
            windows,
        );
        let imported = FastActorSessionV1::import_infoset_v1(&live, live.game_state().clone(), 0);
        assert_eq!(
            imported.surface.harness_surface().action_window_seats_v1(),
            windows
        );
    }
}
