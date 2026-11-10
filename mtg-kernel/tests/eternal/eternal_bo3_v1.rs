//! Eternal best-of-three (stage S2): snapshot decks whose whole 75 is Full
//! play matches through public sessions with a seeded random sideboarding
//! policy, and the format-aware registration, the companion-aware game
//! builder and the match driver keep their rules.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{fully_supported, play_with_outside, snapshot_decks, Policy};
use mtg_kernel::bo3_match::{GameOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1};
use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::eternal_bo3_v1::{
    EternalBestOfThreeMatchV1, EternalBo3ErrorV1, EternalConfigurationV1, EternalFormatV1,
    EternalRegisteredDeckV1, RandomSideboardPolicyV1,
};
use mtg_kernel::ids::PlayerId;
use mtg_kernel::limited_session_v1::CustomCardCountV1;
use std::collections::{BTreeMap, BTreeSet};

/// Snapshot archetypes whose mainboard, companion and sideboard are all Full.
/// A wave that completes a deck's sideboard adds it here; a regression
/// removes one and fails.
const PLAYABLE_75: [(&str, &str); 49] = [
    ("pauper", "pauper-mono-red-madness"),
    (
        "pauper",
        "pauper-blue-terror-2b4f6710-ccb2-4660-b5f9-ea85c8875ec5",
    ),
    ("pauper", "elves-67156d3a-d400-4269-8400-3a0258a274b3"),
    (
        "pauper",
        "pauper-dimir-faeries-625747e4-1ba6-4401-af7b-5c8033a12c6c",
    ),
    ("pauper", "bogles-df219f8f-1b57-437c-b4d8-dd4a5c7bdd7c"),
    ("pauper", "pauper-mono-red-rally"),
    ("pauper", "pauper-esper-affinity"),
    ("pauper", "pauper-ruby-storm"),
    ("pauper", "pauper-golgari-gardens"),
    (
        "pauper",
        "pauper-dimir-control-1e2279c1-d135-4609-8b99-83f2ab98c424",
    ),
    ("pauper", "pauper-familiars"),
    ("pauper", "pauper-esper-ephemerate"),
    (
        "pauper",
        "pauper-turbo-fog-fe589501-15e6-4511-b259-e066f9f36c51",
    ),
    (
        "pauper",
        "pauper-mono-blue-faeries-f7d232c7-2c3c-4459-b219-f3e8d9abbec6",
    ),
    ("pauper", "pauper-dimir-affinity"),
    ("pauper", "pauper-gruul-ponza"),
    ("pauper", "pauper-rakdos-madness"),
    ("pauper", "pauper-mardu-synthesizer"),
    ("pauper", "pauper-food-gardens"),
    ("pauper", "pauper-glintblade"),
    ("pauper", "burn-3854fe58-5ff9-4046-bac9-ae9617c3caf5"),
    ("pauper", "pauper-dimir-terror"),
    ("pauper", "pauper-azorius-gates"),
    ("pauper", "pauper-esper-faeries"),
    ("legacy", "legacy-dimir-tempo"),
    ("legacy", "legacy-izzet-delver"),
    ("legacy", "legacy-doomsday"),
    ("legacy", "legacy-aluren"),
    ("legacy", "legacy-the-epic-storm"),
    ("legacy", "legacy-stiflenought"),
    ("legacy", "legacy-affinity-stompy"),
    ("legacy", "legacy-esper-control"),
    (
        "legacy",
        "legacy-death-and-taxes-yorion-mono-white-d867cd0f-702e-45a2-a765-734174ae83d3",
    ),
    ("legacy", "legacy-reanimator"),
    ("legacy", "legacy-8-cast"),
    ("vintage", "vintage-esper-lurrus-control"),
    ("vintage", "vintage-dimir-lurrus-control"),
    ("vintage", "vintage-lurrus-po"),
    ("vintage", "vintage-counter-vine"),
    ("vintage", "vintage-jeskai-lurrus"),
    ("vintage", "vintage-blue-tinker"),
    ("vintage", "vintage-stiflenought"),
    ("vintage", "vintage-beseech-storm"),
    ("vintage", "vintage-lurrus-drs"),
    ("vintage", "vintage-red-prison"),
    ("vintage", "vintage-sultai-psychic-frog"),
    ("vintage", "vintage-4c-psychic-frog"),
    ("vintage", "vintage-eldrazi"),
    ("vintage", "vintage-grixis-lurrus-control"),
];

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} is in the eternal registry"))
}

fn ids(rows: &[(&str, usize)]) -> Vec<u16> {
    rows.iter()
        .flat_map(|&(name, count)| std::iter::repeat_n(id(name), count))
        .collect()
}

fn rows(cards: &[(&str, u32)]) -> Vec<CustomCardCountV1> {
    cards
        .iter()
        .map(|&(name, count)| CustomCardCountV1 {
            name: name.to_owned(),
            count,
        })
        .collect()
}

/// Every snapshot deck's registration, with its format.
fn registered_snapshot_decks() -> Vec<(String, EternalRegisteredDeckV1)> {
    snapshot_decks()
        .into_iter()
        .filter(|deck| PLAYABLE_75.contains(&(deck.format.as_str(), deck.archetype.as_str())))
        .map(|deck| {
            let format = EternalFormatV1::from_snapshot_key(&deck.format).unwrap();
            let registered = EternalRegisteredDeckV1::from_rows_v1(
                format,
                deck.archetype.clone(),
                &deck.deck.cards,
                &deck.sideboard,
                deck.deck.companion.as_deref(),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", deck.archetype));
            (deck.format, registered)
        })
        .collect()
}

#[test]
fn full75_snapshot_decks_match_the_tracked_list() {
    let playable: BTreeSet<(String, String)> = snapshot_decks()
        .iter()
        .filter(|deck| {
            deck.deck
                .cards
                .iter()
                .chain(&deck.sideboard)
                .map(|card| card.name.as_str())
                .chain(deck.deck.companion.as_deref())
                .all(fully_supported)
        })
        .map(|deck| (deck.format.clone(), deck.archetype.clone()))
        .collect();
    let expected: BTreeSet<(String, String)> = PLAYABLE_75
        .iter()
        .map(|(format, archetype)| (format.to_string(), archetype.to_string()))
        .collect();
    assert_eq!(playable, expected);
}

/// Every snapshot list, playable or not, fits its format's deck size bounds,
/// and a companion is listed among the sideboard's cards.
#[test]
fn snapshot_lists_fit_their_format_bounds() {
    for deck in snapshot_decks() {
        let format = EternalFormatV1::from_snapshot_key(&deck.format).unwrap();
        let mainboard: u32 = deck.deck.cards.iter().map(|row| row.count).sum();
        let sideboard: u32 = deck.sideboard.iter().map(|row| row.count).sum();
        assert!(
            mainboard as usize >= format.minimum_mainboard(),
            "{}: {mainboard}",
            deck.archetype
        );
        assert!(
            sideboard as usize <= format.maximum_sideboard(),
            "{}: {sideboard}",
            deck.archetype
        );
        if let Some(companion) = &deck.deck.companion {
            assert!(
                deck.sideboard.iter().any(|row| &row.name == companion),
                "{}: companion {companion} is not in the sideboard",
                deck.archetype
            );
        }
    }
}

/// A 60-card Lurrus mainboard (every permanent card has mana value 2 or
/// less) and a 15-card sideboard holding Lurrus and two cards that break its
/// condition if they come in.
fn lurrus_deck() -> (Vec<u16>, Vec<u16>) {
    (
        ids(&[
            ("Island", 20),
            ("Brainstorm", 4),
            ("Ponder", 4),
            ("Force of Will", 4),
            ("Counterspell", 4),
            ("Mishra's Bauble", 4),
            ("Psychic Frog", 4),
            ("Snapcaster Mage", 4),
            ("Daze", 4),
            ("Lightning Bolt", 4),
            ("Gush", 4),
        ]),
        ids(&[
            ("Lurrus of the Dream-Den", 1),
            ("Pyroblast", 4),
            ("Hydroblast", 4),
            ("Null Rod", 2),
            ("Murktide Regent", 2),
            ("Oko, Thief of Crowns", 2),
        ]),
    )
}

#[test]
fn registration_enforces_format_bounds_copies_support_and_companion() {
    let (main, side) = lurrus_deck();
    let lurrus = id("Lurrus of the Dream-Den");
    let deck = EternalRegisteredDeckV1::new_v1(
        EternalFormatV1::Vintage,
        "lurrus",
        main.clone(),
        side.clone(),
        Some(lurrus),
    )
    .unwrap();
    assert_eq!(deck.companion(), Some(lurrus));
    assert_eq!(deck.registered_configuration().mainboard().len(), 60);

    // Fewer than 60 mainboard cards, more than 15 sideboard cards.
    let mut short = main.clone();
    short.pop();
    assert!(matches!(
        EternalRegisteredDeckV1::new_v1(EternalFormatV1::Legacy, "a", short, side.clone(), None),
        Err(EternalBo3ErrorV1::MainboardTooSmall {
            actual: 59,
            minimum: 60
        })
    ));
    let mut long_side = side.clone();
    long_side.push(id("Island"));
    assert!(matches!(
        EternalRegisteredDeckV1::new_v1(
            EternalFormatV1::Legacy,
            "a",
            main.clone(),
            long_side,
            None
        ),
        Err(EternalBo3ErrorV1::SideboardTooLarge {
            actual: 16,
            maximum: 15
        })
    ));
    // A 61-card mainboard and a 14-card sideboard are within bounds, as are
    // more than four basic lands.
    let mut big = main.clone();
    big.push(id("Island"));
    let mut small_side = side.clone();
    small_side.retain(|&card| card != lurrus);
    EternalRegisteredDeckV1::new_v1(EternalFormatV1::Pauper, "b", big, small_side, None).unwrap();

    // A fifth copy across the 75 counts both zones.
    let mut fifth = side.clone();
    fifth.retain(|&card| card != id("Null Rod"));
    fifth.push(id("Brainstorm"));
    fifth.push(id("Island"));
    assert!(matches!(
        EternalRegisteredDeckV1::new_v1(EternalFormatV1::Legacy, "c", main.clone(), fifth, None),
        Err(EternalBo3ErrorV1::CopyLimitExceeded { count: 5, .. })
    ));

    // A companion must be in the sideboard, be a companion and have its
    // condition met by the registered mainboard.
    let mut no_lurrus = side.clone();
    no_lurrus.retain(|&card| card != lurrus);
    no_lurrus.push(id("Island"));
    assert!(matches!(
        EternalRegisteredDeckV1::new_v1(
            EternalFormatV1::Vintage,
            "d",
            main.clone(),
            no_lurrus,
            Some(lurrus)
        ),
        Err(EternalBo3ErrorV1::CompanionNotInSideboard { .. })
    ));
    assert!(matches!(
        EternalRegisteredDeckV1::new_v1(
            EternalFormatV1::Vintage,
            "e",
            main.clone(),
            side.clone(),
            Some(id("Pyroblast"))
        ),
        Err(EternalBo3ErrorV1::NotACompanion { .. })
    ));
    assert_eq!(
        EternalRegisteredDeckV1::new_v1(
            EternalFormatV1::Vintage,
            "bad companion id",
            main.clone(),
            side.clone(),
            Some(u16::MAX),
        ),
        Err(EternalBo3ErrorV1::UnknownCardId { card_id: u16::MAX })
    );
    let mut three_drop = main.clone();
    three_drop.retain(|&card| card != id("Daze"));
    three_drop.extend(ids(&[("Daze", 3), ("Brazen Borrower", 1)]));
    // Brazen Borrower is a permanent card with mana value 3.
    assert!(matches!(
        EternalRegisteredDeckV1::new_v1(
            EternalFormatV1::Vintage,
            "f",
            three_drop,
            side.clone(),
            Some(lurrus)
        ),
        Err(EternalBo3ErrorV1::CompanionConditionNotMet { .. })
    ));

    // Tokens and unknown names are refused; rows of one card add up.
    let mut token = main.clone();
    token.pop();
    token.push(id("Treasure Token"));
    assert!(matches!(
        EternalRegisteredDeckV1::new_v1(EternalFormatV1::Legacy, "g", token, side.clone(), None),
        Err(EternalBo3ErrorV1::UnsupportedCard { .. })
    ));
    let split = EternalRegisteredDeckV1::from_rows_v1(
        EternalFormatV1::Legacy,
        "h",
        &rows(&[("Island", 30), ("Island", 26), ("Brainstorm", 4)]),
        &rows(&[("Hydroblast", 1), ("Hydroblast", 2)]),
        None,
    )
    .unwrap();
    assert_eq!(
        split.registered_configuration().sideboard(),
        &[id("Hydroblast"); 3]
    );
    assert!(matches!(
        EternalRegisteredDeckV1::from_rows_v1(
            EternalFormatV1::Legacy,
            "i",
            &rows(&[("Island", 60)]),
            &rows(&[("No Such Card", 1)]),
            None
        ),
        Err(EternalBo3ErrorV1::UnknownCard { .. })
    ));
    assert!(matches!(
        EternalRegisteredDeckV1::from_rows_v1(
            EternalFormatV1::Legacy,
            "oversized mainboard",
            &rows(&[("Island", u32::MAX)]),
            &[],
            None,
        ),
        Err(EternalBo3ErrorV1::MainboardTooLarge { .. })
    ));
    assert!(matches!(
        EternalRegisteredDeckV1::from_rows_v1(
            EternalFormatV1::Legacy,
            "oversized sideboard",
            &rows(&[("Island", 60)]),
            &rows(&[("Island", u32::MAX)]),
            None,
        ),
        Err(EternalBo3ErrorV1::SideboardTooLarge { .. })
    ));
}

#[test]
fn game_builder_uses_the_companion_only_while_it_qualifies() {
    let (main, side) = lurrus_deck();
    let lurrus = id("Lurrus of the Dream-Den");
    let deck = EternalRegisteredDeckV1::new_v1(
        EternalFormatV1::Vintage,
        "lurrus",
        main.clone(),
        side.clone(),
        Some(lurrus),
    )
    .unwrap();

    // Registered: Lurrus is the companion and the other 14 cards are
    // outside the game.
    let game = deck
        .game_deck_v1(1, deck.registered_configuration())
        .unwrap();
    assert_eq!(
        game.deck.companion.as_deref(),
        Some("Lurrus of the Dream-Den")
    );
    assert_eq!(game.outside_the_game.len(), 14);
    assert!(!game.outside_the_game.contains(&lurrus));
    assert_eq!(game.deck.cards.iter().map(|row| row.count).sum::<u32>(), 60);

    // Bringing in Murktide Regent (mana value 7) breaks the condition, so
    // Lurrus is an ordinary sideboard card this game.
    let swap = |out: &str, inn: &str| {
        let mut main = deck.registered_configuration().mainboard().to_vec();
        let mut side = deck.registered_configuration().sideboard().to_vec();
        let at = main.iter().position(|&card| card == id(out)).unwrap();
        let from = side.iter().position(|&card| card == id(inn)).unwrap();
        std::mem::swap(&mut main[at], &mut side[from]);
        EternalConfigurationV1::new_v1(main, side)
    };
    let murktide = swap("Daze", "Murktide Regent");
    deck.admit_configuration_v1(2, &murktide).unwrap();
    let game = deck.game_deck_v1(2, &murktide).unwrap();
    assert_eq!(game.deck.companion, None);
    assert_eq!(game.outside_the_game.len(), 15);
    assert!(game.outside_the_game.contains(&lurrus));

    // A qualifying exchange keeps the companion.
    let pyroblast = swap("Daze", "Pyroblast");
    let game = deck.game_deck_v1(2, &pyroblast).unwrap();
    assert_eq!(
        game.deck.companion.as_deref(),
        Some("Lurrus of the Dream-Den")
    );
    assert!(game.outside_the_game.contains(&id("Daze")));

    // Lurrus brought into the mainboard is not a companion.
    let in_main = swap("Daze", "Lurrus of the Dream-Den");
    let game = deck.game_deck_v1(2, &in_main).unwrap();
    let invalid = EternalConfigurationV1::new_v1(vec![u16::MAX; 60], Vec::new());
    assert!(matches!(
        deck.game_deck_v1(2, &invalid),
        Err(EternalBo3ErrorV1::RegisteredCardsChanged { .. })
    ));
    assert_eq!(game.deck.companion, None);
    assert!(game
        .deck
        .cards
        .iter()
        .any(|row| row.name == "Lurrus of the Dream-Den"));

    // Game one must use the registered configuration, and no game may
    // change the 75 cards or leave the bounds.
    assert!(matches!(
        deck.admit_configuration_v1(1, &pyroblast),
        Err(EternalBo3ErrorV1::GameOneSideboarded { .. })
    ));
    let mut changed = deck.registered_configuration().mainboard().to_vec();
    // Black Lotus is in neither zone of this deck.
    changed[0] = id("Black Lotus");
    assert!(matches!(
        deck.admit_configuration_v1(
            2,
            &EternalConfigurationV1::new_v1(
                changed,
                deck.registered_configuration().sideboard().to_vec()
            )
        ),
        Err(EternalBo3ErrorV1::RegisteredCardsChanged { .. })
    ));
    let mut fewer = deck.registered_configuration().mainboard().to_vec();
    let mut more = deck.registered_configuration().sideboard().to_vec();
    more.push(fewer.pop().unwrap());
    assert!(matches!(
        deck.admit_configuration_v1(2, &EternalConfigurationV1::new_v1(fewer, more)),
        Err(EternalBo3ErrorV1::MainboardTooSmall { .. })
    ));
}

#[test]
fn match_seats_the_starting_player_first_and_refuses_bad_configurations() {
    let decks = registered_snapshot_decks();
    let pauper: Vec<&EternalRegisteredDeckV1> = decks
        .iter()
        .filter(|(format, _)| format == "pauper")
        .map(|(_, deck)| deck)
        .collect();
    let legacy = decks
        .iter()
        .find(|(format, _)| format == "legacy")
        .map(|(_, deck)| deck.clone())
        .unwrap();
    assert!(matches!(
        EternalBestOfThreeMatchV1::new_v1([pauper[0].clone(), legacy], PlayerId::P0),
        Err(EternalBo3ErrorV1::FormatMismatch)
    ));

    let mut bo3 =
        EternalBestOfThreeMatchV1::new_v1([pauper[0].clone(), pauper[1].clone()], PlayerId::P1)
            .unwrap();
    let registered = [PlayerId::P0, PlayerId::P1]
        .map(|player| bo3.deck(player).registered_configuration().clone());
    // The game one chooser draws: player 0 starts and sits in seat 0.
    let game = bo3
        .start_game_v1(PlayerId::P1, PlayDrawChoiceV1::Draw, registered.clone())
        .unwrap();
    assert_eq!(game.start().starting_player, PlayerId::P0);
    assert_eq!(game.player_in_seat(0), PlayerId::P0);
    assert_eq!(game.session_decks()[0], game.game_deck(PlayerId::P0).deck);
    bo3.record_game_result_v1(GameOutcomeV1::Win {
        winner: PlayerId::P0,
    })
    .unwrap();

    // Game two: the loser (player 1) chooses to play and sits in seat 0. A
    // configuration that changes the 75 is refused and leaves the match
    // waiting for the same choice.
    let mut changed = registered.clone();
    let mut main = changed[1].mainboard().to_vec();
    main[0] = id("Black Lotus");
    changed[1] = EternalConfigurationV1::new_v1(main, changed[1].sideboard().to_vec());
    let before = bo3.match_state().clone();
    assert!(bo3
        .start_game_v1(PlayerId::P1, PlayDrawChoiceV1::Play, changed)
        .is_err());
    assert_eq!(bo3.match_state(), &before);
    let policy = RandomSideboardPolicyV1 {
        seed: 7,
        max_exchanges: 5,
    };
    let configurations =
        [PlayerId::P0, PlayerId::P1].map(|player| policy.configure_v1(bo3.deck(player), player, 2));
    let game = bo3
        .start_game_v1(PlayerId::P1, PlayDrawChoiceV1::Play, configurations)
        .unwrap();
    assert_eq!(game.start().game_index, 2);
    assert_eq!(game.player_in_seat(0), PlayerId::P1);
    assert_eq!(game.session_decks()[1], game.game_deck(PlayerId::P0).deck);
}

#[test]
fn native_opening_match_uses_real_london_keeps_and_preserves_the_profile_both_seats() {
    use mtg_kernel::engine::{self, Action, Decision};
    use mtg_kernel::eternal_opening_v1::RecordV1;
    use mtg_kernel::eternal_outside_game_v1::outside_the_game;
    use mtg_kernel::event::CommittedEvent;
    use mtg_kernel::state::{GameState, Step, Target, Zone};

    let [p0, p1] = [PlayerId::P0, PlayerId::P1];
    let void = id("Leyline of the Void");
    let sanctity = id("Leyline of Sanctity");
    let chancellor = id("Chancellor of the Annex");
    let companion = id("Yorion, Sky Nomad");
    let bolt = id("Lightning Bolt");
    let dart = id("Lava Dart");
    let make_main = |basic| {
        [
            vec![basic; 68],
            vec![void; 4],
            vec![sanctity; 4],
            vec![chancellor; 4],
        ]
        .concat()
    };
    let format = EternalFormatV1::Legacy;
    let main = make_main(id("Island"));
    assert!(matches!(
        EternalRegisteredDeckV1::new_v1(format, "legacy", main.clone(), vec![], None),
        Err(EternalBo3ErrorV1::UnsupportedCard { .. })
    ));
    let mut unrelated = main.clone();
    unrelated[0] = id("Meddling Mage");
    assert!(matches!(
        EternalRegisteredDeckV1::new_with_native_opening_v1(
            format,
            "unrelated",
            unrelated,
            vec![],
            None
        ),
        Err(EternalBo3ErrorV1::UnsupportedCard { .. })
    ));
    let zero = EternalRegisteredDeckV1::new_with_native_opening_v1(
        format,
        "zero",
        main,
        vec![companion, bolt],
        Some(companion),
    )
    .unwrap();
    let one = EternalRegisteredDeckV1::new_with_native_opening_v1(
        format,
        "one",
        make_main(id("Swamp")),
        vec![dart],
        None,
    )
    .unwrap();
    let legacy =
        EternalRegisteredDeckV1::new_v1(format, "legacy", vec![id("Island"); 60], vec![], None)
            .unwrap();
    assert!(matches!(
        EternalBestOfThreeMatchV1::new_v1([legacy.clone(), one.clone()], p0),
        Err(EternalBo3ErrorV1::OpeningProfileMismatch)
    ));
    let mut ordinary =
        EternalBestOfThreeMatchV1::new_v1([legacy.clone(), legacy.clone()], p0).unwrap();
    assert!(matches!(
        ordinary.start_game_with_native_opening_v1(
            p0,
            PlayDrawChoiceV1::Play,
            [
                legacy.registered_configuration().clone(),
                legacy.registered_configuration().clone()
            ],
            12,
        ),
        Err(EternalBo3ErrorV1::OpeningProfileMismatch)
    ));

    for starting_match_player in [p0, p1] {
        let mut bo3 =
            EternalBestOfThreeMatchV1::new_v1([zero.clone(), one.clone()], starting_match_player)
                .unwrap();
        let configurations = [
            zero.registered_configuration().clone(),
            one.registered_configuration().clone(),
        ];
        let before = bo3.match_state().clone();
        assert!(matches!(
            bo3.start_game_v1(
                starting_match_player,
                PlayDrawChoiceV1::Play,
                configurations.clone()
            ),
            Err(EternalBo3ErrorV1::OpeningProfileMismatch)
        ));
        assert_eq!(bo3.match_state(), &before);

        // This shuffle gives native seat 0 Void and Sanctity after one
        // mulligan, and seat 1 Sanctity and Chancellor in its original seven.
        let (game, mut state) = bo3
            .start_game_with_native_opening_v1(
                starting_match_player,
                PlayDrawChoiceV1::Play,
                configurations.clone(),
                12,
            )
            .unwrap();
        assert_eq!(game.player_in_seat(0), starting_match_player);
        assert_eq!(state.starting_player, p0);
        for seat in [p0, p1] {
            let match_player = game.player_in_seat(seat.index());
            assert_eq!(state.players[seat.index()].hand.len(), 7);
            assert_eq!(
                outside_the_game(&state, seat),
                if match_player == p0 { &[bolt] } else { &[dart] }
            );
        }
        assert_eq!(state.exile.len(), 1);
        let companion_object = state.objects.get(state.exile[0]);
        assert_eq!(companion_object.card_def, companion);
        assert_eq!(game.player_in_seat(companion_object.owner.index()), p0);

        for (seat, mulligan) in [(p0, true), (p1, false)] {
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::ChooseLondonMulligan { player, mulligan_count: 0 } if player == seat));
            engine::step(&mut state, Action::ChooseLondonMulligan { mulligan }).unwrap();
        }
        let Decision::ChooseLondonBottom {
            player,
            remaining: 1,
            candidates,
        } = engine::advance_until_decision(&mut state)
        else {
            panic!("expected the real London bottom after the redraw");
        };
        assert_eq!(player, p0);
        let bottom = candidates
            .into_iter()
            .find(|&card| state.objects.get(card).card_def == sanctity)
            .unwrap();
        engine::step(&mut state, Action::ChooseLondonBottom(bottom)).unwrap();
        assert_eq!(state.players[0].library.last(), Some(&bottom));
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseLondonMulligan { player, mulligan_count: 1 } if player == p0));
        engine::step(&mut state, Action::ChooseLondonMulligan { mulligan: false }).unwrap();
        assert_eq!(state.players[0].hand.len(), 6);

        let mut state: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for (seat, card) in [(p0, void), (p1, sanctity)] {
            let Decision::ChooseEternalOpeningV1 {
                player,
                legal_cards,
            } = engine::advance_until_decision(&mut state)
            else {
                panic!("expected a native kept-hand opening choice");
            };
            assert_eq!(player, seat);
            assert_eq!(state.step, Step::Untap);
            assert!(state.stack.is_empty());
            assert!(!legal_cards.contains(&bottom));
            assert!(legal_cards
                .iter()
                .all(|&id| state.objects.get(id).owner == seat));
            let chosen = legal_cards
                .iter()
                .copied()
                .find(|&id| state.objects.get(id).card_def == card)
                .unwrap();
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(chosen)),
            )
            .unwrap();
            assert_eq!(state.objects.get(chosen).zone, Zone::Battlefield);
            if seat == p1 {
                let reveal = legal_cards
                    .into_iter()
                    .find(|&id| state.objects.get(id).card_def == chancellor)
                    .unwrap();
                assert!(matches!(engine::advance_until_decision(&mut state),
                    Decision::ChooseEternalOpeningV1 { player, legal_cards } if player == p1 && legal_cards.contains(&reveal)));
                engine::step(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(reveal)),
                )
                .unwrap();
                assert_eq!(state.objects.get(reveal).zone, Zone::Hand);
                assert!(state
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event,
                    CommittedEvent::EternalOpeningV1 { record: RecordV1::Revealed(source) }
                        if source.source == reveal)));
            }
            assert!(
                matches!(engine::advance_until_decision(&mut state), Decision::ChooseEternalOpeningV1 { player, .. } if player == seat)
            );
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        }
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));

        bo3.record_game_result_v1(GameOutcomeV1::Win {
            winner: starting_match_player,
        })
        .unwrap();
        let mut sideboarded = configurations;
        let mut new_main = sideboarded[1].mainboard().to_vec();
        let basic = new_main[0];
        new_main[0] = dart;
        sideboarded[1] = EternalConfigurationV1::new_v1(new_main, vec![basic]);
        let next_starter = starting_match_player.opponent();
        let (next, mut state) = bo3
            .start_game_with_native_opening_v1(
                next_starter,
                PlayDrawChoiceV1::Play,
                sideboarded,
                12,
            )
            .unwrap();
        assert_eq!(next.start().game_index, 2);
        assert_eq!(next.player_in_seat(0), next_starter);
        let one_seat = if next.player_in_seat(0) == p1 { p0 } else { p1 };
        assert_eq!(outside_the_game(&state, one_seat), &[basic]);
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseLondonMulligan { player, mulligan_count: 0 } if player == p0)
        );
    }
}

#[test]
fn random_sideboarding_is_seeded_and_conserves_the_registered_75() {
    let (main, side) = lurrus_deck();
    let lurrus = id("Lurrus of the Dream-Den");
    let deck = EternalRegisteredDeckV1::new_v1(
        EternalFormatV1::Vintage,
        "lurrus",
        main,
        side,
        Some(lurrus),
    )
    .unwrap();
    let mut exchanged = 0;
    for seed in 0..64 {
        let policy = RandomSideboardPolicyV1 {
            seed,
            max_exchanges: 5,
        };
        assert_eq!(
            &policy.configure_v1(&deck, PlayerId::P0, 1),
            deck.registered_configuration()
        );
        for game_index in [2, 3] {
            let configuration = policy.configure_v1(&deck, PlayerId::P1, game_index);
            assert_eq!(
                configuration,
                policy.configure_v1(&deck, PlayerId::P1, game_index)
            );
            deck.admit_configuration_v1(game_index, &configuration)
                .unwrap();
            assert!(configuration.sideboard().contains(&lurrus));
            let (came_in, went_out) = configuration.exchanges_from(deck.registered_configuration());
            assert_eq!(came_in.len(), went_out.len());
            assert!(came_in.len() <= 5);
            exchanged += came_in.len();
        }
    }
    assert!(exchanged > 0);
}

/// One best-of-three match between two registrations: each game's
/// transcript hash, plus what the games exercised.
struct MatchRecord {
    transcripts: Vec<String>,
    games: usize,
    sideboarded_games: usize,
    sided_in_casts: usize,
    companion_games: usize,
    winner: Option<PlayerId>,
}

/// Plays a match with seeded random actions and the seeded random
/// sideboarding policy. Each game's play/draw chooser (the game one
/// chooser, then the loser of the previous game) chooses to play.
fn play_match(decks: [EternalRegisteredDeckV1; 2], seed: u64) -> MatchRecord {
    let policy = RandomSideboardPolicyV1 {
        seed,
        max_exchanges: 5,
    };
    let mut bo3 = EternalBestOfThreeMatchV1::new_v1(decks, PlayerId((seed % 2) as u8)).unwrap();
    let mut record = MatchRecord {
        transcripts: Vec::new(),
        games: 0,
        sideboarded_games: 0,
        sided_in_casts: 0,
        companion_games: 0,
        winner: None,
    };
    loop {
        let (game_index, chooser) = match bo3.match_state().phase() {
            MatchPhaseV1::AwaitingPlayDrawChoice {
                game_index,
                chooser,
            } => (game_index, chooser),
            MatchPhaseV1::Complete { outcome } => {
                record.winner = match outcome {
                    mtg_kernel::bo3_match::MatchOutcomeV1::Winner { winner } => Some(winner),
                    mtg_kernel::bo3_match::MatchOutcomeV1::Draw => None,
                };
                return record;
            }
            MatchPhaseV1::AwaitingGameResult { .. } => unreachable!("results are recorded"),
        };
        assert!(game_index <= 5, "seed={seed}: too many drawn games");
        let configurations = [PlayerId::P0, PlayerId::P1]
            .map(|player| policy.configure_v1(bo3.deck(player), player, game_index));
        let game = bo3
            .start_game_v1(chooser, PlayDrawChoiceV1::Play, configurations)
            .unwrap();
        let game_seed = seed.wrapping_mul(8).wrapping_add(u64::from(game_index));
        // The game's sideboard (without a companion in use) is outside the
        // game, where wish and learn effects find it.
        let played = play_with_outside(
            game.session_decks(),
            Some(game.session_outside_the_game()),
            game_seed,
            Policy::Random(game_seed),
        );
        record.games += 1;
        record.transcripts.push(played.transcript_sha256);
        let mut exchanges = 0;
        for seat in 0..2 {
            let player = game.player_in_seat(seat);
            let (came_in, _) = game
                .configuration(player)
                .exchanges_from(bo3.deck(player).registered_configuration());
            exchanges += came_in.len();
            let came_in: BTreeSet<u16> = came_in.into_iter().collect();
            record.sided_in_casts += played.cast_cards[seat]
                .iter()
                .filter(|(card, _)| came_in.contains(card))
                .map(|(_, casts)| casts)
                .sum::<usize>();
            record.companion_games += usize::from(game.game_deck(player).deck.companion.is_some());
        }
        record.sideboarded_games += usize::from(exchanges > 0);
        let outcome = match played.winner {
            Some(seat) => GameOutcomeV1::Win {
                winner: game.player_in_seat(seat),
            },
            None => GameOutcomeV1::Draw,
        };
        bo3.record_game_result_v1(outcome).unwrap();
    }
}

#[test]
fn playable_75_decks_play_best_of_three_with_random_sideboarding() {
    let decks = registered_snapshot_decks();
    assert_eq!(decks.len(), PLAYABLE_75.len());
    let mut by_format: BTreeMap<&str, Vec<&EternalRegisteredDeckV1>> = BTreeMap::new();
    for (format, deck) in &decks {
        by_format.entry(format.as_str()).or_default().push(deck);
    }
    let mut failures = Vec::new();
    let (mut games, mut sideboarded, mut sided_in_casts, mut companion_games) = (0, 0, 0, 0);
    let mut index = 0u64;
    for (format, decks) in &by_format {
        // Each deck plays the next deck of its format, or a mirror when the
        // format has one deck.
        for (position, first) in decks.iter().enumerate() {
            let second = decks[(position + 1) % decks.len()];
            let pair = [(*first).clone(), second.clone()];
            let seed = 0x424f_3300_0000 + index;
            index += 1;
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let record = play_match(pair.clone(), seed);
                let replay = play_match(pair.clone(), seed);
                assert_eq!(record.transcripts, replay.transcripts);
                assert_eq!(record.winner, replay.winner);
                record
            }));
            let Ok(record) = outcome else {
                failures.push(format!(
                    "{format}: {} / {} seed={seed}",
                    first.deck_id(),
                    second.deck_id()
                ));
                continue;
            };
            println!(
                "{} / {} seed={seed} games={} sideboarded={} sided_in_casts={} winner={:?} transcripts={:?}",
                first.deck_id(),
                second.deck_id(),
                record.games,
                record.sideboarded_games,
                record.sided_in_casts,
                record.winner,
                record.transcripts
            );
            games += record.games;
            sideboarded += record.sideboarded_games;
            sided_in_casts += record.sided_in_casts;
            companion_games += record.companion_games;
        }
    }
    assert!(
        failures.is_empty(),
        "failing matches:\n{}",
        failures.join("\n")
    );
    assert!(games >= 2 * decks.len(), "{games} games");
    assert!(sideboarded > 0, "no game was sideboarded");
    assert!(sided_in_casts > 0, "no sideboard card was cast");
    let companions = decks.iter().any(|(_, deck)| deck.companion().is_some());
    assert!(
        !companions || companion_games > 0,
        "no game used a companion"
    );
    // Every card of a playable 75 is Full in the catalog.
    for (_, deck) in &decks {
        let configuration = deck.registered_configuration();
        assert!(configuration
            .mainboard()
            .iter()
            .chain(configuration.sideboard())
            .all(|&card| CARD_DEFS[usize::from(card)].has_full_support()));
    }
}
