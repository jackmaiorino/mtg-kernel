use mtg_kernel::bo3_match::{GameOutcomeV1, MatchOutcomeV1, PlayDrawChoiceV1};
use mtg_kernel::bo3_session::{BestOfThreeDeckMatchV1, Bo3SessionErrorV1};
use mtg_kernel::ids::PlayerId;
use mtg_kernel::sideboard::{DeckConfigurationV1, SideboardErrorV1};

fn configurations(session: &BestOfThreeDeckMatchV1) -> [DeckConfigurationV1; 2] {
    [PlayerId::P0, PlayerId::P1].map(|seat| {
        session
            .registered_deck(seat)
            .unwrap()
            .registered_configuration()
            .clone()
    })
}

fn swap_one(configuration: &DeckConfigurationV1) -> DeckConfigurationV1 {
    let mut main = configuration.mainboard().to_vec();
    let mut side = configuration.sideboard().to_vec();
    let side_index = side.iter().position(|id| *id != main[0]).unwrap();
    std::mem::swap(&mut main[0], &mut side[side_index]);
    DeckConfigurationV1::new_exact_v1(main, side).unwrap()
}

#[test]
fn live_cross_deck_configurations_apply_to_both_seats_and_preserve_chooser() {
    let mut session =
        BestOfThreeDeckMatchV1::checked_in_pauper_v1("Rally", "Affinity", PlayerId::P0).unwrap();
    let registered = configurations(&session);
    session
        .prepare_game_with_configurations_v1(
            PlayerId::P0,
            PlayDrawChoiceV1::Play,
            registered.clone(),
        )
        .unwrap();
    session
        .record_game_result_v1(GameOutcomeV1::Win {
            winner: PlayerId::P0,
        })
        .unwrap();
    let changed = registered.each_ref().map(swap_one);
    let game = session
        .prepare_game_with_configurations_v1(PlayerId::P1, PlayDrawChoiceV1::Play, changed.clone())
        .unwrap();
    assert_eq!(game.start().game_index, 2);
    assert_eq!(game.start().starting_player, PlayerId::P1);
    for (seat, expected) in [PlayerId::P0, PlayerId::P1].into_iter().zip(&changed) {
        assert_eq!(game.configuration(seat), Some(expected));
        assert!(game.sideboard_receipt(seat).is_none());
    }
    session
        .record_game_result_v1(GameOutcomeV1::Win {
            winner: PlayerId::P0,
        })
        .unwrap();
    assert_eq!(
        session.match_state().outcome(),
        Some(MatchOutcomeV1::Winner {
            winner: PlayerId::P0
        })
    );
}

#[test]
fn game_one_sideboarding_rejected_without_advancing_phase() {
    let mut session =
        BestOfThreeDeckMatchV1::checked_in_pauper_v1("Terror", "Rally", PlayerId::P1).unwrap();
    let before = session.clone();
    let mut selected = configurations(&session);
    selected[1] = swap_one(&selected[1]);
    assert_eq!(
        session.prepare_game_with_configurations_v1(PlayerId::P1, PlayDrawChoiceV1::Draw, selected),
        Err(Bo3SessionErrorV1::GameOneConfigurationChanged {
            player: PlayerId::P1
        })
    );
    assert_eq!(session, before);
}

#[test]
fn invalid_second_seat_inventory_rolls_back_valid_first_seat() {
    let mut session =
        BestOfThreeDeckMatchV1::checked_in_pauper_v1("Rally", "Terror", PlayerId::P0).unwrap();
    session
        .prepare_game_with_configurations_v1(
            PlayerId::P0,
            PlayDrawChoiceV1::Play,
            configurations(&session),
        )
        .unwrap();
    session.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
    let before = session.clone();
    let mut selected = configurations(&session).each_ref().map(swap_one);
    let mut main = selected[1].mainboard().to_vec();
    main[0] = u16::MAX;
    selected[1] =
        DeckConfigurationV1::new_exact_v1(main, selected[1].sideboard().to_vec()).unwrap();
    assert_eq!(
        session.prepare_game_with_configurations_v1(PlayerId::P0, PlayDrawChoiceV1::Play, selected),
        Err(Bo3SessionErrorV1::Sideboard(
            SideboardErrorV1::RegisteredMultisetChanged
        ))
    );
    assert_eq!(session, before);
}

#[test]
fn repeated_draws_keep_live_configuration_through_game_four() {
    let mut session =
        BestOfThreeDeckMatchV1::checked_in_pauper_v1("Elves", "Terror", PlayerId::P1).unwrap();
    let registered = configurations(&session);
    for index in 1..=4 {
        let selected = if index == 1 {
            registered.clone()
        } else {
            registered.each_ref().map(swap_one)
        };
        let game = session
            .prepare_game_with_configurations_v1(
                PlayerId::P1,
                PlayDrawChoiceV1::Play,
                selected.clone(),
            )
            .unwrap();
        assert_eq!(game.start().game_index, index);
        assert_eq!(game.configuration(PlayerId::P0), Some(&selected[0]));
        session.record_game_result_v1(GameOutcomeV1::Draw).unwrap();
    }
}
