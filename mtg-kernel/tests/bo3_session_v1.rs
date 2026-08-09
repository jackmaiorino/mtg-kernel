use mtg_kernel::bo3_match::{GameOutcomeV1, MatchPhaseV1, MatchTransitionV1, PlayDrawChoiceV1};
use mtg_kernel::bo3_session::{BestOfThreeDeckMatchV1, Bo3SessionErrorV1};
use mtg_kernel::ids::PlayerId;
use mtg_kernel::sideboard::{
    CardCountV1, DeterministicSideboardPolicyV1, RegisteredDeckV1, SideboardDefaultPlanV1,
    SideboardErrorV1, SideboardPlanV1, SideboardZoneV1,
};

fn deck_a_v1() -> RegisteredDeckV1 {
    let mut mainboard = vec![10; 4];
    mainboard.extend(std::iter::repeat_n(11, 56));
    let mut sideboard = vec![20; 3];
    sideboard.extend(std::iter::repeat_n(21, 12));
    RegisteredDeckV1::new_exact_v1("A", mainboard, sideboard).unwrap()
}

fn deck_b_v1() -> RegisteredDeckV1 {
    let mut mainboard = vec![30; 2];
    mainboard.extend(std::iter::repeat_n(31, 58));
    let mut sideboard = vec![40; 2];
    sideboard.extend(std::iter::repeat_n(41, 13));
    RegisteredDeckV1::new_exact_v1("B", mainboard, sideboard).unwrap()
}

fn policy_v1() -> DeterministicSideboardPolicyV1 {
    DeterministicSideboardPolicyV1::new_v1(
        "pair-policy/v1",
        vec!["A".to_owned(), "B".to_owned()],
        SideboardDefaultPlanV1::KeepRegisteredConfiguration,
        vec![
            SideboardPlanV1::new_v1(
                "A",
                "B",
                2,
                vec![CardCountV1 {
                    card_id: 20,
                    count: 3,
                }],
                vec![CardCountV1 {
                    card_id: 10,
                    count: 3,
                }],
            )
            .unwrap(),
            SideboardPlanV1::new_v1(
                "B",
                "A",
                2,
                vec![CardCountV1 {
                    card_id: 40,
                    count: 2,
                }],
                vec![CardCountV1 {
                    card_id: 30,
                    count: 2,
                }],
            )
            .unwrap(),
        ],
    )
    .unwrap()
}

#[test]
fn game_one_uses_registration_and_game_two_applies_both_ordered_plans() {
    let mut session =
        BestOfThreeDeckMatchV1::new_v1(deck_a_v1(), deck_b_v1(), policy_v1(), PlayerId::P0)
            .unwrap();

    let game_one = session
        .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(game_one.start().game_index, 1);
    assert_eq!(game_one.start().starting_player, PlayerId::P0);
    assert_eq!(
        game_one.configuration(PlayerId::P0).unwrap(),
        session
            .registered_deck(PlayerId::P0)
            .unwrap()
            .registered_configuration()
    );
    assert!(game_one.sideboard_receipt(PlayerId::P0).is_none());
    assert!(game_one.sideboard_receipt(PlayerId::P1).is_none());

    assert_eq!(
        session
            .record_game_result_v1(GameOutcomeV1::Win {
                winner: PlayerId::P0,
            })
            .unwrap(),
        MatchTransitionV1::NextGameChoice {
            game_index: 2,
            chooser: PlayerId::P1,
        }
    );
    let game_two = session
        .prepare_game_v1(PlayerId::P1, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(game_two.start().game_index, 2);
    assert_eq!(game_two.start().starting_player, PlayerId::P1);
    assert_eq!(
        game_two
            .configuration(PlayerId::P0)
            .unwrap()
            .mainboard()
            .iter()
            .filter(|card_id| **card_id == 20)
            .count(),
        3
    );
    assert_eq!(
        game_two
            .configuration(PlayerId::P1)
            .unwrap()
            .mainboard()
            .iter()
            .filter(|card_id| **card_id == 40)
            .count(),
        2
    );
    for player in [PlayerId::P0, PlayerId::P1] {
        let receipt = game_two.sideboard_receipt(player).unwrap();
        assert_eq!(receipt.game_index(), 2);
        assert_eq!(receipt.policy_id(), "pair-policy/v1");
    }
}

#[test]
fn policy_failure_is_transactional_with_respect_to_match_phase() {
    let invalid_policy = DeterministicSideboardPolicyV1::new_v1(
        "invalid-at-application/v1",
        vec!["A".to_owned(), "B".to_owned()],
        SideboardDefaultPlanV1::KeepRegisteredConfiguration,
        vec![SideboardPlanV1::new_v1(
            "A",
            "B",
            2,
            vec![CardCountV1 {
                card_id: 99,
                count: 1,
            }],
            vec![CardCountV1 {
                card_id: 10,
                count: 1,
            }],
        )
        .unwrap()],
    )
    .unwrap();
    let mut session =
        BestOfThreeDeckMatchV1::new_v1(deck_a_v1(), deck_b_v1(), invalid_policy, PlayerId::P0)
            .unwrap();
    session
        .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
        .unwrap();
    session
        .record_game_result_v1(GameOutcomeV1::Win {
            winner: PlayerId::P0,
        })
        .unwrap();
    let before = session.match_state().clone();

    assert_eq!(
        session.prepare_game_v1(PlayerId::P1, PlayDrawChoiceV1::Play),
        Err(Bo3SessionErrorV1::Sideboard(
            SideboardErrorV1::CardUnavailable {
                zone: SideboardZoneV1::RegisteredSideboard,
                card_id: 99,
                requested: 1,
                available: 0,
            }
        ))
    );
    assert_eq!(session.match_state(), &before);
    assert_eq!(
        session.match_state().phase(),
        MatchPhaseV1::AwaitingPlayDrawChoice {
            game_index: 2,
            chooser: PlayerId::P1,
        }
    );
}

#[test]
fn mirror_deck_id_requires_one_registered_seventy_five() {
    let first = RegisteredDeckV1::new_exact_v1("Mirror", vec![1; 60], vec![2; 15]).unwrap();
    let second = RegisteredDeckV1::new_exact_v1("Mirror", vec![3; 60], vec![4; 15]).unwrap();
    let policy = DeterministicSideboardPolicyV1::new_v1(
        "mirror-policy/v1",
        vec!["Mirror".to_owned()],
        SideboardDefaultPlanV1::KeepRegisteredConfiguration,
        vec![],
    )
    .unwrap();
    assert_eq!(
        BestOfThreeDeckMatchV1::new_v1(first, second, policy, PlayerId::P0),
        Err(Bo3SessionErrorV1::MirrorRegistrationMismatch {
            deck_id: "Mirror".to_owned(),
        })
    );
}

#[test]
fn identical_inputs_prepare_identical_games_and_receipts() {
    fn run() -> mtg_kernel::bo3_session::PreparedMatchGameV1 {
        let mut session =
            BestOfThreeDeckMatchV1::new_v1(deck_a_v1(), deck_b_v1(), policy_v1(), PlayerId::P0)
                .unwrap();
        session
            .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Draw)
            .unwrap();
        session.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
        session
            .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
            .unwrap()
    }
    assert_eq!(run(), run());
}

#[test]
fn completed_checked_in_decks_create_an_executable_postboard_session() {
    let mut session =
        BestOfThreeDeckMatchV1::checked_in_pauper_v1("Terror", "Terror", PlayerId::P0).unwrap();
    let game_one = session
        .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
        .unwrap();
    assert_eq!(
        game_one
            .configuration(PlayerId::P0)
            .unwrap()
            .mainboard()
            .len(),
        60
    );
    assert!(game_one.sideboard_receipt(PlayerId::P0).is_none());

    assert_eq!(
        session
            .record_game_result_v1(GameOutcomeV1::Win {
                winner: PlayerId::P0,
            })
            .unwrap(),
        MatchTransitionV1::NextGameChoice {
            game_index: 2,
            chooser: PlayerId::P1,
        }
    );
    let game_two = session
        .prepare_game_v1(PlayerId::P1, PlayDrawChoiceV1::Play)
        .unwrap();
    for player in [PlayerId::P0, PlayerId::P1] {
        assert_eq!(
            game_two.configuration(player).unwrap().mainboard().len(),
            60
        );
        assert_eq!(
            game_two.configuration(player).unwrap().sideboard().len(),
            15
        );
        assert!(game_two.sideboard_receipt(player).is_some());
    }
}

#[test]
fn drawn_games_can_reach_a_fourth_postboard_game() {
    let mut session =
        BestOfThreeDeckMatchV1::checked_in_pauper_v1("Terror", "Terror", PlayerId::P0).unwrap();
    for game_index in 1..=3 {
        let game = session
            .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
            .unwrap();
        assert_eq!(game.start().game_index, game_index);
        assert_eq!(
            session.record_game_result_v1(GameOutcomeV1::Draw).unwrap(),
            MatchTransitionV1::NextGameChoice {
                game_index: game_index + 1,
                chooser: PlayerId::P0,
            }
        );
    }

    let game_four = session
        .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Draw)
        .unwrap();
    assert_eq!(game_four.start().game_index, 4);
    assert_eq!(game_four.start().starting_player, PlayerId::P1);
    for player in [PlayerId::P0, PlayerId::P1] {
        assert_eq!(
            game_four.configuration(player).unwrap().mainboard().len(),
            60
        );
        assert!(game_four.sideboard_receipt(player).is_some());
    }
}
