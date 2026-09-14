// Test-only verbatim pre-refactor fast loop, retained as the behavior oracle.
use super::*;
use crate::human_opening_v1::{HumanOpeningPhaseV1, HumanOpeningV1};
use crate::paired_bo1_harness_v1::{
    policy_test_support::SeededRandomBo1PolicyV1, PairedBo1PolicyInputV1, PairedBo1PolicyV1,
};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};

fn tags() -> RemovalCounterspellTagsV1 {
    RemovalCounterspellTagsV1 { requires_target: Default::default(), is_counterspell: Default::default() }
}

fn actual_opening(human: PlayerId, starting: PlayerId, opponent_land: &str) -> HumanOpeningV1 {
    let mountain = crate::card_def::card_id_by_name("Mountain").unwrap();
    let other = crate::card_def::card_id_by_name(opponent_land).unwrap();
    let mut decks = [vec![mountain; 60], vec![mountain; 60]];
    decks[human.opponent().index()] = vec![other; 60];
    HumanOpeningV1::new(11, 99, 10_000, 100_000, ["First".into(), "Second".into()], decks, starting, human).unwrap()
}

fn actual_session(starting: PlayerId) -> FastActorSessionV1 {
    let mut opening = actual_opening(PlayerId::P0, starting, "Island");
    opening.keep().unwrap();
    opening.into_session().unwrap()
}

fn policy() -> SeededRandomBo1PolicyV1 {
    let mut p = SeededRandomBo1PolicyV1::default();
    p.reset_for_game_v1([101, 102]).unwrap();
    p
}

fn one_step(session: &mut FastActorSessionV1, policy: &mut SeededRandomBo1PolicyV1) {
    let FastActorResponseV1::Decision(d) = session.current_response() else { panic!("expected decision") };
    let selected = policy.select_action_v1(PairedBo1PolicyInputV1::new(session, d)).unwrap();
    session.step(d.episode_id, d.step, selected).unwrap();
}

fn incremental_to_natural(session: &mut FastActorSessionV1, p: &mut SeededRandomBo1PolicyV1) -> CompletedGameSummaryV2 {
    let mut accumulated = FastGameSummaryAccumulatorV1::new_v1(session).unwrap();
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(_) => return accumulated.finish_natural_v1(session, "weights", &tags()).unwrap(),
            FastActorResponseV1::Decision(_) => {
                assert!(accumulated.observe_current_v1(session).unwrap());
                assert!(!accumulated.observe_current_v1(session).unwrap());
                assert!(!accumulated.observe_current_v1(session).unwrap());
                one_step(session, p);
            }
        }
    }
}

#[test]
fn natural_incremental_and_automatic_match_frozen_driver_bytes_rng_and_actions() {
    // Genuine all-basic engine games terminate by drawing from an empty deck.
    // No policy scorer, sampler, natural winner or terminal is fabricated.
    for starting in [PlayerId::P0, PlayerId::P1] {
        let mut old = actual_session(starting);
        let mut automatic = actual_session(starting);
        let mut interactive = actual_session(starting);
        let (mut p0, mut p1, mut p2) = (policy(), policy(), policy());
        let expected = frozen_fast_driver_oracle_v1(&mut old, "weights", &tags(), &mut p0).unwrap();
        let actual = try_run_fast_episode_with_summary_v1(&mut automatic, "weights", &tags(), &mut p1).unwrap();
        let completed = incremental_to_natural(&mut interactive, &mut p2);
        assert_eq!(completed.completion, GameSummaryCompletionV2::Natural);
        assert_eq!(completed.history.initial_policy_steps, 0);
        assert_eq!(completed.history.observed_gameplay_decisions, completed.history.committed_policy_steps);
        assert!(completed.history.committed_policy_steps > 0);
        assert_eq!(serde_json::to_vec(&expected).unwrap(), serde_json::to_vec(&actual).unwrap());
        assert_eq!(serde_json::to_vec(&expected).unwrap(), serde_json::to_vec(&completed.summary).unwrap());
        assert_eq!(p0.traces, p1.traces);
        assert_eq!(p0.traces, p2.traces);
        assert_eq!(old.current_response(), automatic.current_response());
        assert_eq!(old.current_response(), interactive.current_response());
        assert_eq!(serde_json::to_vec(old.game_state()).unwrap(), serde_json::to_vec(interactive.game_state()).unwrap());
    }
}

#[test]
fn legacy_partial_start_keeps_its_old_summary_but_full_history_constructor_rejects_it() {
    let mut old = actual_session(PlayerId::P0);
    let mut fresh = actual_session(PlayerId::P0);
    let (mut p0, mut p1) = (policy(), policy());
    for _ in 0..5 { one_step(&mut old, &mut p0); one_step(&mut fresh, &mut p1); }
    assert!(FastGameSummaryAccumulatorV1::new_v1(&fresh).is_err());
    let expected = frozen_fast_driver_oracle_v1(&mut old, "weights", &tags(), &mut p0).unwrap();
    let actual = try_run_fast_episode_with_summary_v1(&mut fresh, "weights", &tags(), &mut p1).unwrap();
    assert_eq!(serde_json::to_vec(&expected).unwrap(), serde_json::to_vec(&actual).unwrap());
    assert_eq!(p0.traces, p1.traces);
}

#[test]
fn opening_concessions_bind_actual_mulligan_bottom_ready_history_without_gameplay() {
    let mountain = crate::card_def::card_id_by_name("Mountain").unwrap();
    for human in [PlayerId::P0, PlayerId::P1] {
        for starting in [PlayerId::P0, PlayerId::P1] {
            let mut opening = actual_opening(human, starting, "Island");
            for phase_index in 0..4 {
                match phase_index {
                    1 => opening.mulligan().unwrap(),
                    2 => opening.keep().unwrap(),
                    3 => opening.bottom(&[0]).unwrap(),
                    _ => {}
                }
                let before = serde_json::to_vec(opening.summary_state_v2().unwrap().0).unwrap();
                let completed = finish_opening_concession_v2(&opening, human, "weights", &tags()).unwrap();
                let again = finish_opening_concession_v2(&opening, human, "weights", &tags()).unwrap();
                assert_eq!(serde_json::to_vec(&completed).unwrap(), serde_json::to_vec(&again).unwrap());
                assert_eq!(before, serde_json::to_vec(opening.summary_state_v2().unwrap().0).unwrap());
                assert_eq!(completed.summary.winner, Some(human.opponent()));
                assert_eq!(completed.history.observed_gameplay_decisions, 0);
                assert_eq!(completed.history.committed_policy_steps, 0);
                assert_eq!(completed.history.committed_physical_decisions, 0);
                assert!(completed.history.retained_event_count >= 14);
                let expected_phase = [HumanOpeningPhaseV1::Mulligan, HumanOpeningPhaseV1::Mulligan, HumanOpeningPhaseV1::Bottom, HumanOpeningPhaseV1::Ready][phase_index];
                let GameSummaryCompletionV2::Concession { conceding_player, stage: GameConcessionStageV2::Opening { phase, human_seat, starting_player, mulligans_taken, hand_counts, library_counts } } = completed.completion else { panic!("explicit opening concession required") };
                assert_eq!(conceding_player, human);
                assert_eq!(human_seat, human.into());
                assert_eq!(starting_player, starting.into());
                assert_eq!(phase, expected_phase);
                assert_eq!(mulligans_taken, u8::from(phase_index > 0));
                assert_eq!(hand_counts[human.index()], if phase_index == 3 { 6 } else { 7 });
                assert_eq!(hand_counts[human.opponent().index()], 7);
                assert_eq!(library_counts[human.index()] + hand_counts[human.index()], 60);
                let own = &completed.summary.own_card_outcomes[human.index()][&mountain];
                assert_eq!(own.times_drawn, if phase_index == 0 { 7 } else { 14 });
                assert!(own.stuck_in_hand);
                assert!(!own.cast && !own.died_without_dealing_damage && !own.removal_no_target);
                assert!(completed.summary.opponent_evidence.iter().all(Vec::is_empty));
                assert_eq!(serde_json::to_vec(&completed.summary.resource_curve).unwrap(), serde_json::to_vec(&ResourceCurveV1::default()).unwrap());
            }
        }
    }
}

#[test]
fn opening_concession_projection_hides_other_seat_hand_and_library_identities() {
    let mountain = crate::card_def::card_id_by_name("Mountain").unwrap();
    let registered = crate::sideboard::DeckConfigurationV1::new_exact_v1(vec![mountain;60],vec![mountain;15]).unwrap();
    for human in [PlayerId::P0, PlayerId::P1] {
        let mut projected = Vec::new();
        for opponent_land in ["Island", "Swamp"] {
            let opening = actual_opening(human, PlayerId::P0, opponent_land);
            let completed = finish_opening_concession_v2(&opening, human, "weights", &tags()).unwrap();
            let mut wins = [0, 0];
            wins[human.opponent().index()] = 1;
            projected.push(crate::learned_bo3_v1::project_sideboard_input_v1(&registered, human, &[completed.summary], 2, wins).unwrap());
        }
        assert_eq!(projected[0], projected[1]);
        assert!(projected[0].opponent_evidence.is_empty());
        assert!(projected[0].resource_summaries[0].own_hand_mean.is_none());
    }
}

#[test]
fn actual_offered_hand_cast_is_retained_once_before_a_gameplay_concession() {
    let mainboards = ["Burn", "Rally"].map(|name| {
        crate::runtime_decks::runtime_deck_by_id(name).unwrap().card_ids.to_vec()
    });
    let mut opening = HumanOpeningV1::new(21, 99, 5_000, 50_000, ["Burn".into(), "Rally".into()], mainboards, PlayerId::P0, PlayerId::P0).unwrap();
    opening.keep().unwrap();
    let mut session = opening.into_session().unwrap();
    let mut accumulator = FastGameSummaryAccumulatorV1::new_v1(&session).unwrap();
    let mut p = policy();
    for _ in 0..1_000 {
        let FastActorResponseV1::Decision(decision) = session.current_response() else { panic!("expected a real offered cast before terminal") };
        let offered = session.current_offered_hand_cast_ids_v1();
        accumulator.observe_current_v1(&session).unwrap();
        assert!(!accumulator.observe_current_v1(&session).unwrap());
        if let Some(&card_id) = offered.first() {
            let actor = PlayerId(seat_index_v1(decision.acting_player) as u8);
            assert!(accumulator.history.offered_as_cast[actor.index()].contains(&card_id));
            // An explicitly synthetic tag isolates the held-without-offer hook.
            // The offered hand card and its legal action come from the engine.
            let mut tagged = tags();
            tagged.is_counterspell.insert(card_id);
            let completed = accumulator.finish_concession_v1(&session, actor, "weights", &tagged).unwrap();
            let own = &completed.summary.own_card_outcomes[actor.index()][&card_id];
            assert!(own.stuck_in_hand);
            assert!(!own.counterspell_held);
            assert_eq!(completed.history.observed_gameplay_decisions, session.policy_step_count() + 1);
            return;
        }
        one_step(&mut session, &mut p);
    }
    panic!("fixed actual-game prefix did not offer a hand cast within its bound");
}

#[test]
fn gameplay_concession_keeps_actual_prefix_and_rejects_unobserved_or_terminal_histories() {
    let mut session = actual_session(PlayerId::P0);
    let mut accumulator = FastGameSummaryAccumulatorV1::new_v1(&session).unwrap();
    let mut p = policy();
    for _ in 0..4 { accumulator.observe_current_v1(&session).unwrap(); one_step(&mut session, &mut p); }
    let before = serde_json::to_vec(session.game_state()).unwrap();
    let completed = accumulator.finish_concession_v1(&session, PlayerId::P1, "weights", &tags()).unwrap();
    assert_eq!(before, serde_json::to_vec(session.game_state()).unwrap());
    assert_eq!(completed.summary.winner, Some(PlayerId::P0));
    assert_eq!(completed.history.committed_policy_steps, 4);
    assert_eq!(completed.history.observed_gameplay_decisions, 5);
    assert_eq!(completed.completion, GameSummaryCompletionV2::Concession { conceding_player: PlayerId::P1, stage: GameConcessionStageV2::Gameplay });
    assert!(!completed.summary.resource_curve.lands_by_turn.is_empty());

    let mut missed = actual_session(PlayerId::P0);
    let accumulator = FastGameSummaryAccumulatorV1::new_v1(&missed).unwrap();
    one_step(&mut missed, &mut policy());
    assert!(accumulator.finish_concession_v1(&missed, PlayerId::P0, "weights", &tags()).is_err());

    let mut capped = FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
        3, 99, 1, 1, ["A".into(),"B".into()], [vec![crate::card_def::card_id_by_name("Mountain").unwrap();60],vec![crate::card_def::card_id_by_name("Island").unwrap();60]], PlayerId::P0,
    ).unwrap();
    let mut accumulated = FastGameSummaryAccumulatorV1::new_v1(&capped).unwrap();
    accumulated.observe_current_v1(&capped).unwrap();
    one_step(&mut capped, &mut policy());
    let FastActorResponseV1::Terminal(terminal) = capped.current_response() else { panic!("one-step cap must halt") };
    assert_ne!(terminal.terminal_classification, crate::rl::TerminalClassificationV1::Natural);
    assert!(accumulated.finish_concession_v1(&capped, PlayerId::P0, "weights", &tags()).is_err());
    assert!(FastGameSummaryAccumulatorV1::for_legacy_driver_v1(&capped).finish_natural_v1(&capped, "weights", &tags()).is_err());
}

fn frozen_fast_driver_oracle_v1(
    session: &mut crate::rl_session::FastActorSessionV1,
    checkpoint_weights_hash: &str,
    tags: &RemovalCounterspellTagsV1,
    policy: &mut dyn crate::paired_bo1_harness_v1::PairedBo1PolicyV1,
) -> Result<GameSummaryV1, String> {
    use crate::paired_bo1_harness_v1::PairedBo1PolicyInputV1;
    use crate::rl_session::FastActorResponseV1;
    let mut offered_as_cast: [std::collections::BTreeSet<u16>; 2] = Default::default();
    let mut resource_curve = ResourceCurveV1::default();
    let mut turn_watermarks = Vec::new();
    let mut last_turn = None;
    loop {
        match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                if terminal.terminal_classification != crate::rl::TerminalClassificationV1::Natural
                {
                    return Err(format!(
                        "non-natural game terminal: {:?}, {:?}: {}",
                        terminal.terminal_classification,
                        terminal.terminal_code,
                        terminal.terminal_reason
                    ));
                }
                let state = session.game_state();
                let object_card_def = build_object_card_def_map_v1(state);
                let object_owner = build_object_owner_map_v1(state);
                let registered = [
                    own_registered_card_ids_v1(state, 0),
                    own_registered_card_ids_v1(state, 1),
                ];
                let hands = [
                    end_of_game_hand_card_ids_v1(state, &object_card_def, 0),
                    end_of_game_hand_card_ids_v1(state, &object_card_def, 1),
                ];
                let (opponent_evidence, own_card_outcomes) = fold_event_history_v1(
                    &state.engine.event_history,
                    state.turn,
                    &turn_watermarks,
                    &object_card_def,
                    &object_owner,
                    &registered,
                    &hands,
                    &offered_as_cast,
                    tags,
                    &mut resource_curve,
                );
                return Ok(GameSummaryV1 {
                    schema_version: GAME_SUMMARY_SCHEMA_V1,
                    checkpoint_weights_hash: checkpoint_weights_hash.to_owned(),
                    checkpoint_git_head: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_owned(),
                    winner: terminal
                        .winner
                        .map(|seat| PlayerId(seat_index_v1(seat) as u8)),
                    opponent_evidence,
                    own_card_outcomes,
                    resource_curve,
                });
            }
            FastActorResponseV1::Decision(decision) => {
                let state = session.game_state();
                if last_turn != Some(state.turn) {
                    turn_watermarks.push((state.engine.event_history.len(), state.turn));
                    resource_curve.lands_by_turn.push(count_lands_v1(state));
                    for seat in 0..2 {
                        resource_curve.hand_size_by_turn[seat]
                            .push(state.players[seat].hand.len() as u32);
                        resource_curve.life_by_turn[seat].push(state.players[seat].life);
                    }
                    last_turn = Some(state.turn);
                }
                offered_as_cast[seat_index_v1(decision.acting_player)]
                    .extend(session.current_offered_hand_cast_ids_v1());
                let selected = policy
                    .select_action_v1(PairedBo1PolicyInputV1::new(session, decision))
                    .map_err(|error| error.to_string())?;
                session
                    .step(decision.episode_id, decision.step, selected)
                    .map_err(|error| {
                        format!("{error}; decision {decision:?}, selected index {selected}")
                    })?;
            }
        }
    }
}

