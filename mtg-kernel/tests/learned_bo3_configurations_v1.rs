use mtg_kernel::bo3_match::{GameOutcomeV1, MatchOutcomeV1, PlayDrawChoiceV1};
use mtg_kernel::bo3_session::{BestOfThreeDeckMatchV1, Bo3SessionErrorV1};
use mtg_kernel::ids::PlayerId;
use mtg_kernel::learned_bo3_v1::LearnedBo3RegistrationRecordV1;
use mtg_kernel::sideboard::{DeckConfigurationV1, RegisteredDeckV1, SideboardErrorV1};

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

fn brewer_registration(label: &str, with_bolt: bool) -> RegisteredDeckV1 {
    let mountain = mtg_kernel::card_def::card_id_by_name("Mountain").unwrap();
    let forest = mtg_kernel::card_def::card_id_by_name("Forest").unwrap();
    let mut mainboard = vec![mountain; 60];
    if with_bolt {
        mainboard[0] = mtg_kernel::card_def::card_id_by_name("Lightning Bolt").unwrap();
    }
    RegisteredDeckV1::new_executable_v1(label, mainboard, vec![forest; 15]).unwrap()
}

#[test]
fn explicit_brewer_registration_needs_no_catalog_or_static_policy() {
    let registrations = [
        brewer_registration("BrewerCandidate", true),
        brewer_registration("BrewerCandidate", false),
    ];
    assert_ne!(
        registrations[0].registered_75_sha256_v1(),
        registrations[1].registered_75_sha256_v1()
    );
    let records = registrations
        .each_ref()
        .map(LearnedBo3RegistrationRecordV1::from_registered_v1);
    let mut session = BestOfThreeDeckMatchV1::new_live_v1(registrations, PlayerId::P0).unwrap();
    assert!(session.sideboard_policy().is_none());
    let before = session.clone();
    assert_eq!(
        session.prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play),
        Err(Bo3SessionErrorV1::LiveConfigurationsRequired)
    );
    assert_eq!(session, before);
    let original = configurations(&session);
    session
        .prepare_game_with_configurations_v1(
            PlayerId::P0,
            PlayDrawChoiceV1::Play,
            original.clone(),
        )
        .unwrap();
    session
        .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
        .unwrap();
    let selected = original.each_ref().map(swap_one);
    let second = session
        .prepare_game_with_configurations_v1(
            PlayerId::P1,
            PlayDrawChoiceV1::Draw,
            selected.clone(),
        )
        .unwrap();
    assert_eq!(second.start().starting_player, PlayerId::P0);
    for (seat, record) in [PlayerId::P0, PlayerId::P1].into_iter().zip(records) {
        let registration = session.registered_deck(seat).unwrap();
        assert_eq!(record, LearnedBo3RegistrationRecordV1::from_registered_v1(registration));
        assert_ne!(
            original[seat.index()].mainboard_sha256_v1(),
            second.configuration(seat).unwrap().mainboard_sha256_v1()
        );
        assert_eq!(record.mainboard, original[seat.index()].mainboard());
        assert_eq!(record.sideboard, original[seat.index()].sideboard());
        assert!(second.sideboard_receipt(seat).is_none());
    }
}

#[test]
fn live_registration_validation_enforces_combined_copy_limit_and_basic_exception() {
    let mountain = mtg_kernel::card_def::card_id_by_name("Mountain").unwrap();
    let bolt = mtg_kernel::card_def::card_id_by_name("Lightning Bolt").unwrap();
    let mut main = vec![mountain; 60];
    main[..4].fill(bolt);
    let mut side = vec![mountain; 15];
    side[0] = bolt;
    // Structural construction cannot bypass the live initializer's admission.
    let unchecked = RegisteredDeckV1::new_exact_v1("TooManyBolts", main.clone(), side.clone()).unwrap();
    let expected = SideboardErrorV1::NonbasicCopyLimitExceeded { card_id: bolt, count: 5 };
    assert_eq!(unchecked.validate_executable_v1(), Err(expected.clone()));
    assert_eq!(
        BestOfThreeDeckMatchV1::new_live_v1(
            [brewer_registration("Valid", false), unchecked],
            PlayerId::P0,
        ),
        Err(Bo3SessionErrorV1::Sideboard(expected))
    );
    main[0] = mountain;
    RegisteredDeckV1::new_executable_v1("FourAcrossZones", main, side).unwrap();
    RegisteredDeckV1::new_executable_v1("BasicException", vec![mountain; 60], vec![mountain; 15]).unwrap();
}

#[test]
fn live_registration_validation_rejects_wrong_size_unknown_cards_and_tokens() {
    let mountain = mtg_kernel::card_def::card_id_by_name("Mountain").unwrap();
    let token = mtg_kernel::card_def::card_id_by_name("Treasure Token").unwrap();
    assert!(matches!(
        RegisteredDeckV1::new_executable_v1("Short", vec![mountain; 59], vec![mountain; 15]),
        Err(SideboardErrorV1::WrongMainboardSize { actual: 59 })
    ));
    let mut unknown = vec![mountain; 15];
    unknown[0] = u16::MAX;
    assert!(matches!(
        RegisteredDeckV1::new_executable_v1("Unknown", vec![mountain; 60], unknown),
        Err(SideboardErrorV1::UnknownRegisteredCardId { .. })
    ));
    let mut tokens = vec![mountain; 15];
    tokens[0] = token;
    assert!(matches!(
        RegisteredDeckV1::new_executable_v1("Token", vec![mountain; 60], tokens),
        Err(SideboardErrorV1::TokenInRegisteredDeck { .. })
    ));
}
