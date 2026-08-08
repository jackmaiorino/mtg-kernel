use mtg_kernel::bo3_match::{
    BestOfThreeMatchStateV1, GameOutcomeV1, MatchOutcomeV1, MatchPhaseV1, MatchStateErrorV1,
    MatchTransitionV1, PlayDrawChoiceV1,
};
use mtg_kernel::ids::PlayerId;

#[test]
fn two_zero_terminates_without_starting_game_three() {
    let mut state = BestOfThreeMatchStateV1::new_v1(PlayerId::P0).unwrap();
    let game_one = state
        .choose_play_draw_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(game_one.starting_player, PlayerId::P0);
    assert_eq!(
        state
            .record_game_result_v1(GameOutcomeV1::Win {
                winner: PlayerId::P0,
            })
            .unwrap(),
        MatchTransitionV1::NextGameChoice {
            game_index: 2,
            chooser: PlayerId::P1,
        }
    );

    let game_two = state
        .choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Draw)
        .unwrap();
    assert_eq!(game_two.starting_player, PlayerId::P0);
    assert_eq!(
        state
            .record_game_result_v1(GameOutcomeV1::Win {
                winner: PlayerId::P0,
            })
            .unwrap(),
        MatchTransitionV1::Complete {
            outcome: MatchOutcomeV1::Winner {
                winner: PlayerId::P0,
            },
        }
    );
    assert_eq!(state.games().len(), 2);
    assert_eq!(state.wins(PlayerId::P0).unwrap(), 2);
    assert_eq!(
        state.choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Play),
        Err(MatchStateErrorV1::MatchComplete)
    );
}

#[test]
fn two_one_match_uses_each_games_loser_as_next_chooser() {
    let mut state = BestOfThreeMatchStateV1::new_v1(PlayerId::P0).unwrap();
    state
        .choose_play_draw_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(
        state
            .record_game_result_v1(GameOutcomeV1::Win {
                winner: PlayerId::P0,
            })
            .unwrap(),
        MatchTransitionV1::NextGameChoice {
            game_index: 2,
            chooser: PlayerId::P1,
        }
    );

    state
        .choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(
        state
            .record_game_result_v1(GameOutcomeV1::Win {
                winner: PlayerId::P1,
            })
            .unwrap(),
        MatchTransitionV1::NextGameChoice {
            game_index: 3,
            chooser: PlayerId::P0,
        }
    );

    state
        .choose_play_draw_v1(PlayerId::P0, PlayDrawChoiceV1::Draw)
        .unwrap();
    assert_eq!(
        state
            .record_game_result_v1(GameOutcomeV1::Win {
                winner: PlayerId::P1,
            })
            .unwrap(),
        MatchTransitionV1::Complete {
            outcome: MatchOutcomeV1::Winner {
                winner: PlayerId::P1,
            },
        }
    );
    assert_eq!(state.games().len(), 3);
    assert_eq!(state.wins(PlayerId::P0).unwrap(), 1);
    assert_eq!(state.wins(PlayerId::P1).unwrap(), 2);
}

#[test]
fn after_draw_player_who_was_on_draw_chooses_next() {
    let mut state = BestOfThreeMatchStateV1::new_v1(PlayerId::P0).unwrap();
    let game_one = state
        .choose_play_draw_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(game_one.player_on_draw(), PlayerId::P1);
    assert_eq!(
        state.record_game_result_v1(GameOutcomeV1::Draw).unwrap(),
        MatchTransitionV1::NextGameChoice {
            game_index: 2,
            chooser: PlayerId::P1,
        }
    );

    let game_two = state
        .choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Draw)
        .unwrap();
    assert_eq!(game_two.starting_player, PlayerId::P0);
    assert_eq!(game_two.player_on_draw(), PlayerId::P1);
    assert_eq!(
        state.record_game_result_v1(GameOutcomeV1::Draw).unwrap(),
        MatchTransitionV1::NextGameChoice {
            game_index: 3,
            chooser: PlayerId::P1,
        }
    );

    state
        .choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(
        state.record_game_result_v1(GameOutcomeV1::Draw).unwrap(),
        MatchTransitionV1::Complete {
            outcome: MatchOutcomeV1::Draw,
        }
    );
    assert_eq!(state.wins(PlayerId::P0).unwrap(), 0);
    assert_eq!(state.wins(PlayerId::P1).unwrap(), 0);
    assert_eq!(state.games().len(), 3);
}

#[test]
fn one_win_and_two_draws_produces_a_match_winner_after_game_three() {
    let mut state = BestOfThreeMatchStateV1::new_v1(PlayerId::P0).unwrap();
    state
        .choose_play_draw_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
        .unwrap();
    state.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
    state
        .choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Play)
        .unwrap();
    state
        .record_game_result_v1(GameOutcomeV1::Win {
            winner: PlayerId::P1,
        })
        .unwrap();
    state
        .choose_play_draw_v1(PlayerId::P0, PlayDrawChoiceV1::Draw)
        .unwrap();
    assert_eq!(
        state.record_game_result_v1(GameOutcomeV1::Draw).unwrap(),
        MatchTransitionV1::Complete {
            outcome: MatchOutcomeV1::Winner {
                winner: PlayerId::P1,
            },
        }
    );
}

#[test]
fn phase_errors_are_fail_closed_and_do_not_advance_state() {
    let mut state = BestOfThreeMatchStateV1::new_v1(PlayerId::P0).unwrap();
    assert_eq!(
        BestOfThreeMatchStateV1::new_v1(PlayerId(2)),
        Err(MatchStateErrorV1::InvalidPlayer { actual: 2 })
    );
    assert_eq!(
        state.record_game_result_v1(GameOutcomeV1::Draw),
        Err(MatchStateErrorV1::PlayDrawChoiceRequired)
    );
    assert_eq!(state.games().len(), 0);
    assert_eq!(
        state.choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Play),
        Err(MatchStateErrorV1::WrongChooser {
            expected: PlayerId::P0,
            actual: PlayerId::P1,
        })
    );
    assert_eq!(
        state.phase(),
        MatchPhaseV1::AwaitingPlayDrawChoice {
            game_index: 1,
            chooser: PlayerId::P0,
        }
    );

    state
        .choose_play_draw_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(
        state.choose_play_draw_v1(PlayerId::P0, PlayDrawChoiceV1::Draw),
        Err(MatchStateErrorV1::GameAlreadyStarted)
    );
    assert_eq!(state.games().len(), 0);
    assert_eq!(
        state.record_game_result_v1(GameOutcomeV1::Win {
            winner: PlayerId(9),
        }),
        Err(MatchStateErrorV1::InvalidPlayer { actual: 9 })
    );
    assert_eq!(state.games().len(), 0);
}

#[test]
fn identical_choices_and_results_produce_identical_match_state() {
    fn run() -> BestOfThreeMatchStateV1 {
        let mut state = BestOfThreeMatchStateV1::new_v1(PlayerId::P1).unwrap();
        state
            .choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Draw)
            .unwrap();
        state.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
        state
            .choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Play)
            .unwrap();
        state
            .record_game_result_v1(GameOutcomeV1::Win {
                winner: PlayerId::P0,
            })
            .unwrap();
        state
            .choose_play_draw_v1(PlayerId::P1, PlayDrawChoiceV1::Draw)
            .unwrap();
        state.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
        state
    }
    assert_eq!(run(), run());
}
