//! Eternal best-of-three (stage S2): snapshot decks whose whole 75 is Full
//! play matches through public sessions with a seeded random sideboarding
//! policy, and the format-aware registration, the companion-aware game
//! builder and the match driver keep their rules.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{
    fully_supported, play_with_outside, snapshot_decks, Policy, KNOWN_GAP,
};
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
const PLAYABLE_75: [(&str, &str); 15] = [
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
    ("pauper", "pauper-mono-red-rally"),
    ("pauper", "pauper-esper-affinity"),
    ("pauper", "pauper-golgari-gardens"),
    (
        "pauper",
        "pauper-dimir-control-1e2279c1-d135-4609-8b99-83f2ab98c424",
    ),
    ("pauper", "pauper-rakdos-madness"),
    ("pauper", "burn-3854fe58-5ff9-4046-bac9-ae9617c3caf5"),
    ("pauper", "pauper-azorius-gates"),
    ("pauper", "pauper-esper-faeries"),
    ("legacy", "legacy-doomsday"),
    ("legacy", "legacy-reanimator"),
    ("vintage", "vintage-red-prison"),
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
        .filter(|key| !KNOWN_GAP.contains(&(key.0.as_str(), key.1.as_str())))
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
    let game = deck.game_deck_v1(deck.registered_configuration());
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
    let game = deck.game_deck_v1(&murktide);
    assert_eq!(game.deck.companion, None);
    assert_eq!(game.outside_the_game.len(), 15);
    assert!(game.outside_the_game.contains(&lurrus));

    // A qualifying exchange keeps the companion.
    let pyroblast = swap("Daze", "Pyroblast");
    let game = deck.game_deck_v1(&pyroblast);
    assert_eq!(
        game.deck.companion.as_deref(),
        Some("Lurrus of the Dream-Den")
    );
    assert!(game.outside_the_game.contains(&id("Daze")));

    // Lurrus brought into the mainboard is not a companion.
    let in_main = swap("Daze", "Lurrus of the Dream-Den");
    let game = deck.game_deck_v1(&in_main);
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
