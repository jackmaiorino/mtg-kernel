use super::*;
use crate::event::CommittedEvent;
use crate::human_opening_v1::HumanOpeningV1;
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TerminalClassificationV1};
use crate::runtime_decks::runtime_deck_by_id;
use PauperMulliganModeV1::{KeepSeven, Learned};

const SEED: u64 = 0x5eed_1a0d;

fn mainboards() -> [Vec<u16>; 2] {
    [
        runtime_deck_by_id("Burn").unwrap().card_ids.to_vec(),
        runtime_deck_by_id("Elves").unwrap().card_ids.to_vec(),
    ]
}

fn actor_id(seat: PlayerSeatV1) -> PlayerId {
    flat_player_id_v1(seat)
}

fn decks() -> SessionDeckIdsV1 {
    ["Burn".into(), "Elves".into()]
}

fn london(
    seed: u64,
    starting: PlayerId,
    modes: [PauperMulliganModeV1; 2],
    max_physical: u64,
) -> FastActorSessionV1 {
    FastActorSessionV1::reset_pauper_london_v1(
        7,
        seed,
        max_physical,
        max_physical * 128,
        decks(),
        mainboards(),
        starting,
        modes,
    )
    .unwrap()
}

fn keep_seven_v2(seed: u64, starting: PlayerId) -> FastActorSessionV1 {
    let mut opening = HumanOpeningV1::new(
        7,
        seed,
        256,
        256 * 128,
        decks(),
        mainboards(),
        starting,
        PlayerId::P0,
    )
    .unwrap();
    opening.keep().unwrap();
    opening.into_session().unwrap()
}

fn decision(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(decision) => decision,
        FastActorResponseV1::Terminal(terminal) => panic!("unexpected terminal {terminal:?}"),
    }
}

fn semantics(session: &FastActorSessionV1) -> Vec<ActionSemanticV1> {
    session.diagnostic_current_action_semantics().unwrap()
}

/// The index of the keep (false) or mulligan (true) row of the current
/// London announcement.
fn announce_index(session: &FastActorSessionV1, mulligan: bool) -> u32 {
    semantics(session)
        .iter()
        .position(|semantic| {
            matches!(semantic, ActionSemanticV1::ChooseLondonMulligan { mulligan: m, .. } if *m == mulligan)
        })
        .expect("current decision is a London announcement") as u32
}

fn is_announce(session: &FastActorSessionV1) -> bool {
    session.pauper_london_pending_v1()
        && matches!(
            semantics(session).first(),
            Some(ActionSemanticV1::ChooseLondonMulligan { .. })
        )
}

fn is_bottom(session: &FastActorSessionV1) -> bool {
    session.pauper_london_pending_v1()
        && matches!(
            semantics(session).first(),
            Some(ActionSemanticV1::ChooseLondonBottom { .. })
        )
}

fn step(session: &mut FastActorSessionV1, index: u32) {
    let d = decision(session);
    session.step(d.episode_id, d.step, index).unwrap();
}

/// Answers every pending London decision: `mulligans[seat]` announcements
/// are mulligans, then the seat keeps; bottoms take the first hand card.
fn play_opening(session: &mut FastActorSessionV1, mulligans: [u8; 2]) {
    let mut taken = [0_u8; 2];
    while session.pauper_london_pending_v1() {
        let actor = actor_id(decision(session).acting_player).index();
        if is_announce(session) {
            let mulligan = taken[actor] < mulligans[actor];
            if mulligan {
                taken[actor] += 1;
            }
            let index = announce_index(session, mulligan);
            step(session, index);
        } else {
            assert!(is_bottom(session));
            step(session, 0);
        }
    }
}

fn state_bytes(session: &FastActorSessionV1) -> Vec<u8> {
    serde_json::to_vec(&session.state).unwrap()
}

fn shuffle_ordinals(session: &FastActorSessionV1) -> [u64; 2] {
    let value = serde_json::to_value(&session.state).unwrap();
    serde_json::from_value(
        value["environment_randomization_v2"]["next_live_shuffle_ordinal"].clone(),
    )
    .unwrap()
}

#[test]
fn pauper_london_both_keep_first_in_game_state_equals_keep_seven_v2_both_starting_seats() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        let baseline = keep_seven_v2(SEED, starting);
        for modes in [
            [Learned, Learned],
            [KeepSeven, Learned],
            [KeepSeven, KeepSeven],
        ] {
            let mut session = london(SEED, starting, modes, 256);
            play_opening(&mut session, [0, 0]);
            assert!(!session.pauper_london_pending_v1());
            assert_eq!(
                state_bytes(&session),
                state_bytes(&baseline),
                "{starting:?} {modes:?}"
            );
            assert_eq!(
                session.diagnostic_state_hash(),
                baseline.diagnostic_state_hash()
            );
            assert_eq!(semantics(&session), semantics(&baseline));
            let (london_decision, keep_decision) = (decision(&session), decision(&baseline));
            assert_eq!(london_decision.acting_player, keep_decision.acting_player);
            assert_eq!(
                london_decision.legal_action_count,
                keep_decision.legal_action_count
            );
            let exposed = modes.iter().filter(|mode| **mode == Learned).count() as u64;
            // Only exposed pregame decisions consume counters.
            assert_eq!(london_decision.step, exposed);
            assert_eq!(london_decision.physical_decision_id, exposed);
            assert_eq!(london_decision.environment_revision, exposed);
            if exposed == 0 {
                assert_eq!(session.current_response(), baseline.current_response());
            }
            let record = session.pauper_opening_record_v1().unwrap();
            for seat in 0..2 {
                assert_eq!(record.seats[seat].mode, modes[seat]);
                assert_eq!(record.seats[seat].mulligans_taken, 0);
                assert_eq!(record.seats[seat].kept_hand_size, 7);
                assert!(!record.seats[seat].forced_zero_card_keep);
                assert_eq!(
                    record.seats[seat].exposed_pregame_decisions,
                    u32::from(modes[seat] == Learned)
                );
            }
        }
    }
}

#[test]
fn pauper_london_announcements_precede_redraws_in_starting_player_order() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut session = london(SEED, starting, [Learned, Learned], 256);
        let hands_before = session.state.players.clone().map(|player| player.hand);
        // Starting player announces first.
        assert_eq!(actor_id(decision(&session).acting_player), starting);
        let index = announce_index(&session, true);
        step(&mut session, index);
        // No redraw before the second announcement.
        assert_eq!(
            session.state.players[starting.index()].hand,
            hands_before[starting.index()]
        );
        assert_eq!(
            actor_id(decision(&session).acting_player),
            starting.opponent()
        );
        let view = london_pregame_view_v1(&session.state).unwrap().unwrap();
        assert_eq!(view.phase, LondonPregamePhaseV1::Announce);
        assert!(view.mulligan_this_round[starting.index()]);
        assert_eq!(view.counts, [0, 0]);
        let index = announce_index(&session, true);
        step(&mut session, index);
        // Both hands redraw together, then bottoms run in starting order.
        let view = london_pregame_view_v1(&session.state).unwrap().unwrap();
        assert_eq!(view.phase, LondonPregamePhaseV1::Bottom);
        assert_eq!(view.counts, [1, 1]);
        for player in [PlayerId::P0, PlayerId::P1] {
            assert_eq!(session.state.players[player.index()].hand.len(), 7);
            assert_eq!(session.state.players[player.index()].draws_this_turn, 14);
        }
        assert!(is_bottom(&session));
        assert_eq!(actor_id(decision(&session).acting_player), starting);
        assert_eq!(view.cards_to_bottom(&session.state, starting), 1);
        assert_eq!(semantics(&session).len(), 7);
        step(&mut session, 0);
        assert_eq!(session.state.players[starting.index()].hand.len(), 6);
        assert!(is_bottom(&session));
        assert_eq!(
            actor_id(decision(&session).acting_player),
            starting.opponent()
        );
        step(&mut session, 0);
        // Round two asks the starting player again.
        assert!(is_announce(&session));
        assert_eq!(actor_id(decision(&session).acting_player), starting);
        let view = london_pregame_view_v1(&session.state).unwrap().unwrap();
        assert_eq!(view.mulligan_this_round, [false, false]);
        assert_eq!(view.kept, [false, false]);
        play_opening(&mut session, [0, 0]);
        let record = session.pauper_opening_record_v1().unwrap();
        for seat in 0..2 {
            assert_eq!(record.seats[seat].mulligans_taken, 1);
            assert_eq!(record.seats[seat].kept_hand_size, 6);
            assert_eq!(record.seats[seat].exposed_pregame_decisions, 3);
        }
        // The first in-game decision is the starting player's first turn:
        // no turn-1 draw counter survives the opening.
        assert_eq!(session.state.turn, 1);
        for player in &session.state.players {
            assert_eq!(player.draws_this_turn, 0);
        }
    }
}

#[test]
fn pauper_london_bottom_counts_follow_mulligans_taken() {
    let mut session = london(SEED, PlayerId::P0, [Learned, KeepSeven], 256);
    // P1 keeps automatically; P0 takes three mulligans.
    play_opening(&mut session, [3, 0]);
    let record = session.pauper_opening_record_v1().unwrap().clone();
    assert_eq!(record.seats[0].mulligans_taken, 3);
    assert_eq!(record.seats[0].kept_hand_size, 4);
    // Four announcements and 1 + 2 + 3 bottoms.
    assert_eq!(record.seats[0].exposed_pregame_decisions, 4 + 6);
    assert_eq!(record.seats[1].exposed_pregame_decisions, 0);
    assert_eq!(record.seats[1].kept_hand_size, 7);
    assert_eq!(decision(&session).step, 10);
    assert_eq!(decision(&session).physical_decision_id, 10);
}

#[test]
fn pauper_london_forced_zero_card_keep_completes_without_a_decision() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut session = london(SEED, starting, [Learned, KeepSeven], 256);
        play_opening(&mut session, [7, 0]);
        let record = session.pauper_opening_record_v1().unwrap();
        assert!(record.seats[0].forced_zero_card_keep);
        assert_eq!(record.seats[0].mulligans_taken, 7);
        assert_eq!(record.seats[0].kept_hand_size, 0);
        // Seven announcements and 1 + ... + 7 bottoms; the forced keep is not
        // exposed.
        assert_eq!(record.seats[0].exposed_pregame_decisions, 7 + 28);
        assert!(!record.seats[1].forced_zero_card_keep);
        assert!(session.state.london_mulligans_v1.is_none());
        assert_eq!(session.state.turn, 1);
        assert_eq!(decision(&session).physical_decision_id, 35);
        assert!(session.state.players[0].hand.len() <= 1);
        assert_eq!(
            session.state.players[0].library.len(),
            60 - session.state.players[0].hand.len()
        );
    }
}

#[test]
fn pauper_london_mulligan_shuffles_only_its_owner_ordinal() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let mut modes = [KeepSeven; 2];
        modes[owner.index()] = Learned;
        let mut session = london(SEED, PlayerId::P0, modes, 256);
        let before = shuffle_ordinals(&session);
        let other_library = session.state.players[owner.opponent().index()]
            .library
            .clone();
        let index = announce_index(&session, true);
        step(&mut session, index);
        let after = shuffle_ordinals(&session);
        assert_eq!(after[owner.index()], before[owner.index()] + 1);
        assert_eq!(
            after[owner.opponent().index()],
            before[owner.opponent().index()]
        );
        assert_eq!(
            session.state.players[owner.opponent().index()].library,
            other_library
        );
    }
}

#[test]
fn pauper_london_snapshot_restore_and_replay_at_each_pregame_phase() {
    // A fixed answer script: both mulligan once, P0 bottoms its third card,
    // P1 its first, both keep.
    let script = |session: &FastActorSessionV1, round: &mut [u8; 2]| -> u32 {
        let actor = actor_id(decision(session).acting_player).index();
        if is_announce(session) {
            let mulligan = round[actor] == 0;
            round[actor] += 1;
            announce_index(session, mulligan)
        } else if actor == 0 {
            2
        } else {
            0
        }
    };
    let run = |stop_at: Option<u64>| {
        let mut session = london(SEED, PlayerId::P1, [Learned, Learned], 256);
        let mut round = [0_u8; 2];
        while session.pauper_london_pending_v1() {
            if Some(decision(&session).step) == stop_at {
                return (session, round);
            }
            let index = script(&session, &mut round);
            step(&mut session, index);
        }
        (session, round)
    };
    let (full, _) = run(None);
    // Announce (step 0 and 1) and bottom (steps 2 and 3) phases.
    for stop in [0, 1, 2, 3] {
        let (mut session, mut round) = run(Some(stop));
        let snapshot = session.snapshot_v1();
        let branch_round = round;
        // Diverge: take the other row, then restore.
        let other = if is_announce(&session) {
            announce_index(
                &session,
                round[actor_id(decision(&session).acting_player).index()] != 0,
            )
        } else {
            1
        };
        step(&mut session, other);
        assert_ne!(state_bytes(&session), state_bytes(&snapshot.0));
        session.restore_v1(&snapshot);
        round = branch_round;
        while session.pauper_london_pending_v1() {
            let index = script(&session, &mut round);
            step(&mut session, index);
        }
        assert_eq!(state_bytes(&session), state_bytes(&full));
        assert_eq!(session.current_response(), full.current_response());
        assert_eq!(
            session.pauper_opening_record_v1(),
            full.pauper_opening_record_v1()
        );
    }
    // Replay from the seed reproduces every decision and the first in-game
    // state.
    let (replayed, _) = run(None);
    assert_eq!(state_bytes(&replayed), state_bytes(&full));
}

#[test]
fn pauper_london_stale_and_invalid_actions_are_rejected_without_mutation() {
    let mut session = london(SEED, PlayerId::P0, [Learned, Learned], 256);
    let index = announce_index(&session, true);
    step(&mut session, index);
    let index = announce_index(&session, true);
    step(&mut session, index);
    assert!(is_bottom(&session));
    let before = state_bytes(&session);
    let response = session.current_response();
    let d = decision(&session);
    assert!(session.step(d.episode_id, d.step + 1, 0).is_err());
    assert!(session
        .step(d.episode_id, d.step.wrapping_sub(1), 0)
        .is_err());
    assert!(session.step(d.episode_id + 1, d.step, 0).is_err());
    assert!(session
        .step(d.episode_id, d.step, d.legal_action_count)
        .is_err());
    assert_eq!(state_bytes(&session), before);
    assert_eq!(session.current_response(), response);
    // The engine refuses an action that does not answer the current London
    // decision, before any mutation.
    let mut state = session.state.clone();
    assert!(
        crate::engine::step(&mut state, Action::ChooseLondonMulligan { mulligan: false }).is_err()
    );
    assert!(
        crate::engine::step(&mut state, Action::ChooseLondonBottom(ObjectId(u32::MAX))).is_err()
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn pauper_london_keep_seven_seat_consumes_no_step_id_revision_or_random_draw() {
    // P0 keeps automatically; P1's first announcement is the first exposed
    // decision.
    let session = london(SEED, PlayerId::P0, [KeepSeven, Learned], 256);
    let d = decision(&session);
    assert_eq!(actor_id(d.acting_player), PlayerId::P1);
    assert_eq!(
        (d.step, d.physical_decision_id, d.environment_revision),
        (0, 0, 0)
    );
    let fresh = london(SEED, PlayerId::P0, [Learned, Learned], 256);
    assert_eq!(shuffle_ordinals(&session), shuffle_ordinals(&fresh));
    // Both automatic: the first in-game decision has the keep-seven counters.
    let both = london(SEED, PlayerId::P1, [KeepSeven, KeepSeven], 256);
    assert_eq!(
        both.current_response(),
        keep_seven_v2(SEED, PlayerId::P1).current_response()
    );
    let record = both.pauper_opening_record_v1().unwrap();
    assert!(record
        .seats
        .iter()
        .all(|seat| seat.exposed_pregame_decisions == 0));
}

#[test]
fn pauper_london_decision_caps_count_pregame_rows_and_ids_stay_contiguous() {
    // Two exposed announcements fit in a cap of two; the first in-game
    // decision does not.
    let mut session = london(SEED, PlayerId::P0, [Learned, Learned], 2);
    play_opening(&mut session, [0, 0]);
    let FastActorResponseV1::Terminal(terminal) = session.current_response() else {
        panic!("the physical decision cap must truncate after the pregame rows");
    };
    assert_eq!(
        terminal.terminal_classification,
        TerminalClassificationV1::Truncated
    );
    assert_eq!(terminal.physical_decision_count, 2);
    // A cap inside the opening truncates there.
    let mut session = london(SEED, PlayerId::P0, [Learned, Learned], 3);
    let index = announce_index(&session, true);
    step(&mut session, index);
    let index = announce_index(&session, true);
    step(&mut session, index);
    step(&mut session, 0);
    assert!(matches!(
        session.current_response(),
        FastActorResponseV1::Terminal(_)
    ));
    // Physical ids are contiguous over exposed decisions only, through the
    // first in-game decision.
    let mut session = london(SEED, PlayerId::P1, [Learned, KeepSeven], 512);
    let mut expected = 0;
    let mut round = 0;
    while session.pauper_london_pending_v1() {
        let d = decision(&session);
        assert_eq!((d.physical_decision_id, d.step), (expected, expected));
        expected += 1;
        let index = if is_announce(&session) {
            round += 1;
            announce_index(&session, round <= 2)
        } else {
            0
        };
        session.step(d.episode_id, d.step, index).unwrap();
    }
    let d = decision(&session);
    assert_eq!((d.physical_decision_id, d.step), (expected, expected));
    assert_eq!(
        session.pauper_opening_record_v1().unwrap().seats[0].exposed_pregame_decisions,
        3 + 1 + 2
    );
    assert_eq!(expected, 6);
}

/// Section 3 step 4 of the design: the opening adds only pregame zone
/// changes from hand to library and draws to `event_history`, all before the
/// first `UpkeepBegan`, and leaves nothing in `event_log`.
#[test]
fn pauper_london_pregame_history_holds_only_hand_to_library_moves_and_draws() {
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut session = london(SEED, starting, [Learned, Learned], 256);
        let deal = session.state.engine.event_history.len();
        let baseline = keep_seven_v2(SEED, starting);
        play_opening(&mut session, [2, 1]);
        let history = &session.state.engine.event_history;
        let upkeep = history
            .iter()
            .position(|event| matches!(event, CommittedEvent::UpkeepBegan { .. }))
            .unwrap();
        assert!(deal < upkeep);
        let mut moves = 0;
        let mut draws = 0;
        for event in history.iter().take(upkeep).skip(deal) {
            match event {
                CommittedEvent::ZoneChange {
                    from: Zone::Hand,
                    to: Zone::Library,
                    ..
                } => moves += 1,
                CommittedEvent::Draw {
                    object: Some(_), ..
                } => draws += 1,
                other => panic!("unexpected pregame event {other:?}"),
            }
        }
        // Seven redraws per mulligan; every card that left a hand went to
        // its library, so the moves are the seven dealt plus the redraws
        // minus the kept hands.
        let record = session.pauper_opening_record_v1().unwrap().clone();
        let taken: Vec<usize> = record
            .seats
            .iter()
            .map(|seat| usize::from(seat.mulligans_taken))
            .collect();
        assert_eq!(taken, [2, 1], "{record:?}");
        assert_eq!(draws, 7 * (taken[0] + taken[1]), "{record:?}");
        let kept: usize = record
            .seats
            .iter()
            .map(|seat| usize::from(seat.kept_hand_size))
            .sum();
        assert_eq!(kept, 5 + 6);
        assert_eq!(moves, 14 + draws - kept, "{starting:?} {record:?}");
        // The deal prefix is the keep-seven deal.
        assert_eq!(
            serde_json::to_vec(&history.iter().take(deal).collect::<Vec<_>>()).unwrap(),
            serde_json::to_vec(
                &baseline
                    .state
                    .engine
                    .event_history
                    .iter()
                    .take(deal)
                    .collect::<Vec<_>>()
            )
            .unwrap()
        );
    }
}

#[cfg(feature = "eternal-fixtures")]
#[test]
fn pauper_london_serum_powder_offer_fails_closed() {
    let powder = crate::card_def::card_id_by_name("Serum Powder").unwrap();
    let session = FastActorSessionV1::reset_pauper_london_v1(
        7,
        SEED,
        256,
        256 * 128,
        decks(),
        [vec![powder; 60], mainboards()[1].clone()],
        PlayerId::P0,
        [Learned, Learned],
    )
    .unwrap();
    let FastActorResponseV1::Terminal(terminal) = session.current_response() else {
        panic!("Serum Powder must fail closed");
    };
    assert_eq!(
        terminal.terminal_classification,
        TerminalClassificationV1::Halted
    );
    assert!(terminal
        .terminal_reason
        .starts_with("fail_closed:pauper_london_v1:"));
}
