//! The deck format of the trainer's episodes (`ExpandedEpisodeV1::deck_format`,
//! `sideboard::DeckFormatV1`): the absent field keeps every serialized byte,
//! the exact 60/15 checks and the session; `limited40-v1` admits 40-card
//! decks with an empty or short sideboard at the collector's deck-list check
//! and at the session reset, and still refuses 39 cards; an unknown value is
//! refused by the strict reader. Games here are driven by seeded picks
//! through the collector's own session builder, not a network.

use super::*;
use crate::sideboard::DeckFormatV1::{self, Limited40V1};
use crate::state::SplitMix64;
use PauperMulliganModeV1::Learned;

/// Decisions played per deal.
const MAX_DECISIONS: u64 = 400;
const LIMITED: Option<DeckFormatV1> = Some(Limited40V1);

/// `main` mainboard cards spread evenly over a checked-in Pauper
/// registration's sorted mainboard (a subset, so the copy limit holds and
/// lands keep their share) and its first `side` sideboard cards.
fn deck(id: &str, main: usize, side: usize) -> ExpandedDeckListV1 {
    let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let configuration = registered.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: spread(configuration.mainboard(), main),
        sideboard: configuration.sideboard()[..side].to_vec(),
    }
}

fn spread(cards: &[u16], count: usize) -> Vec<u16> {
    (0..count).map(|i| cards[i * cards.len() / count]).collect()
}

fn pauper(main: usize, side: usize) -> [ExpandedDeckListV1; 2] {
    [deck("Burn", main, side), deck("Elves", main, side)]
}

pub(crate) fn episode(
    lists: [ExpandedDeckListV1; 2],
    london: bool,
    deck_format: Option<DeckFormatV1>,
) -> ExpandedEpisodeV1 {
    ExpandedEpisodeV1 {
        id: "deck-format".into(),
        seed: 0x40_4040,
        starting_player: 1,
        learner_seat: 0,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: lists.clone(),
        selected: lists,
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
        opening: london.then_some(PauperOpeningV1::LondonV1),
        mulligan_modes: london.then_some([Learned, Learned]),
        priority_windows: None,
        london_keep_offset: None,
        deck_format,
    }
}

/// The session `episode` built before the field existed: the reset its
/// builder called then, with the builder's own arguments.
fn session_before_the_field(episode: &ExpandedEpisodeV1) -> FastActorSessionV1 {
    let configs = episode.configurations().unwrap();
    let labels = episode.selected.each_ref().map(|d| d.label.clone());
    let mainboards = configs.each_ref().map(|d| d.mainboard().to_vec());
    let starting = PlayerId(episode.starting_player);
    let windows = [crate::priority_action_windows_v1::SeatPriorityWindowsV1::HarnessV2; 2];
    match episode.mulligan_modes {
        Some(modes) => FastActorSessionV1::reset_pauper_london_windows_v1(
            1,
            episode.seed,
            episode.max_physical_decisions,
            episode.max_policy_steps,
            labels,
            mainboards,
            starting,
            modes,
            windows,
        )
        .unwrap(),
        None => FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_and_windows_v1(
            1,
            episode.seed,
            episode.max_physical_decisions,
            episode.max_policy_steps,
            labels,
            mainboards,
            starting,
            windows,
        )
        .unwrap(),
    }
}

/// The ExpandedEpisodeV1 field order; serde also decodes the struct from a
/// positional array in this order, so `deck_format` must stay last.
const FIELD_ORDER: [&str; 19] = [
    "id",
    "seed",
    "starting_player",
    "learner_seat",
    "opponent",
    "opponent_search",
    "opponent_kind",
    "registered",
    "selected",
    "postboard",
    "max_physical_decisions",
    "max_policy_steps",
    "runtime_rules",
    "reset",
    "opening",
    "mulligan_modes",
    "priority_windows",
    "london_keep_offset",
    "deck_format",
];

fn positional(value: &serde_json::Value, fields: &[&str]) -> serde_json::Value {
    serde_json::Value::Array(
        fields
            .iter()
            .map(|field| {
                value
                    .get(*field)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null)
            })
            .collect(),
    )
}

#[test]
fn deck_format_absent_keeps_every_serialized_byte() {
    let absent = episode(pauper(60, 15), false, None);
    let value = serde_json::to_value(&absent).unwrap();
    assert!(value.get("deck_format").is_none());
    let bytes = serde_json::to_vec(&absent).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("deck_format"));
    let parsed: ExpandedEpisodeV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed, absent);
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);

    // The field writes its name and reads back exactly; without its key the
    // episode is the absent one, byte for byte.
    let present = episode(pauper(60, 15), false, LIMITED);
    let value = serde_json::to_value(&present).unwrap();
    assert_eq!(value["deck_format"], serde_json::json!("limited40-v1"));
    assert_eq!(
        serde_json::from_value::<ExpandedEpisodeV1>(value.clone()).unwrap(),
        present
    );
    let mut stripped = value.clone();
    stripped
        .as_object_mut()
        .unwrap()
        .remove("deck_format")
        .unwrap();
    let stripped: ExpandedEpisodeV1 = serde_json::from_value(stripped).unwrap();
    assert_eq!(stripped, absent);
    assert_eq!(serde_json::to_vec(&stripped).unwrap(), bytes);

    // Positional arrays: the base field list decodes to the absent field,
    // and the field decodes from the last position.
    let base = positional(&value, &FIELD_ORDER[..FIELD_ORDER.len() - 1]);
    assert_eq!(
        serde_json::from_value::<ExpandedEpisodeV1>(base).unwrap(),
        absent
    );
    let full = positional(&value, &FIELD_ORDER);
    assert_eq!(
        serde_json::from_value::<ExpandedEpisodeV1>(full).unwrap(),
        present
    );
}

#[test]
fn deck_format_unknown_value_is_refused() {
    let value = serde_json::to_value(episode(pauper(60, 15), false, LIMITED)).unwrap();
    for bad in [
        serde_json::json!("limited40-v2"),
        serde_json::json!("limited40"),
        serde_json::json!("Limited40V1"),
        serde_json::json!(""),
        serde_json::json!(40),
        serde_json::json!(["limited40-v1"]),
    ] {
        let mut refused = value.clone();
        refused["deck_format"] = bad.clone();
        assert!(
            serde_json::from_value::<ExpandedEpisodeV1>(refused).is_err(),
            "{bad}"
        );
    }
}

/// Without the field a 40-card deck is refused exactly as before, at the
/// collector's deck-list check and, on its own, at the session reset; with
/// it the same episode is admitted and reaches its first decision on both
/// resets, with an empty, short or full sideboard.
#[test]
fn deck_format_limited40_admits_forty_card_decks_only_with_the_field() {
    for london in [false, true] {
        let absent = episode(pauper(40, 0), london, None);
        let error = absent.configurations().unwrap_err();
        assert!(error.contains("must contain exactly 60 cards"), "{error}");
        assert!(error.contains("got 40"), "{error}");

        for side in [0, 7, 15] {
            let present = episode(pauper(40, side), london, LIMITED);
            let configs = present.configurations().unwrap();
            for config in &configs {
                assert_eq!(config.mainboard().len(), 40);
                assert_eq!(config.sideboard().len(), side);
            }
            let session = episode_session_v1(&present, &configs).unwrap();
            assert!(
                matches!(session.current_response(), FastActorResponseV1::Decision(_)),
                "london {london} side {side}"
            );
            // The session reset reads the field itself: the admitted
            // configurations under the absent field are refused there.
            let mut reset_only = present.clone();
            reset_only.deck_format = None;
            let error = episode_session_v1(&reset_only, &configs).err().unwrap();
            assert!(
                error.contains("must contain exactly 60 cards, got 40"),
                "{error}"
            );
        }
    }
}

#[test]
fn deck_format_limited40_still_refuses_39_cards() {
    let short = episode(pauper(39, 0), false, LIMITED);
    let error = short.configurations().unwrap_err();
    assert!(
        error.contains("limited40-v1") && error.contains("got 39"),
        "{error}"
    );

    // One seat, or the selected list only, is enough to refuse.
    let mut one_seat = episode(pauper(40, 0), false, LIMITED);
    one_seat.registered[1] = deck("Elves", 39, 0);
    one_seat.selected[1] = deck("Elves", 39, 0);
    assert!(one_seat.configurations().unwrap_err().contains("got 39"));
    let mut selected_only = episode(pauper(40, 0), false, LIMITED);
    selected_only.postboard = true;
    selected_only.selected[0] = deck("Burn", 39, 1);
    assert!(selected_only
        .configurations()
        .unwrap_err()
        .contains("got 39"));

    // And a sideboard over 15 cards.
    let mut long_side = episode(pauper(40, 15), false, LIMITED);
    for list in [&mut long_side.registered[0], &mut long_side.selected[0]] {
        let extra = list.mainboard.pop().unwrap();
        list.sideboard.push(extra);
        list.mainboard.push(list.mainboard[0]);
    }
    assert!(long_side
        .configurations()
        .unwrap_err()
        .contains("at most 15"));
}

/// The D3 search opponent's simulations are unverified on other decks, so
/// the field is refused with one, before any game; without the field the
/// same episode passes these checks.
#[test]
fn deck_format_is_refused_with_a_search_opponent() {
    let pin = |name: &str| PinnedFileV1 {
        path: PathBuf::from(name),
        sha256: "ab".repeat(32),
    };
    let mut searched = episode(pauper(60, 15), false, LIMITED);
    searched.opponent = Some(ExpandedModelSourceV1 {
        play_import: pin("import.json"),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: None,
        s_learner: None,
        w_learner: None,
    });
    searched.opponent_search = Some(pin("search.json"));
    let error = searched.configurations_checked_v1().unwrap_err();
    assert!(
        error.contains("deck_format") && error.contains("search opponent"),
        "{error}"
    );
    searched.deck_format = None;
    assert!(searched.configurations_checked_v1().is_ok());
}

/// Without the field the builder's session is the one it built before the
/// field existed, at every decision of the same seeded game, on both resets.
/// The field on the same 60/15 decks plays that game too: it changes
/// admission only.
#[test]
fn deck_format_absent_builds_the_plain_session_and_present_plays_the_same_game() {
    for (i, london) in [false, true].into_iter().enumerate() {
        let absent = episode(pauper(60, 15), london, None);
        let present = episode(pauper(60, 15), london, LIMITED);
        let mut before = session_before_the_field(&absent);
        let mut sessions = [
            episode_session_v1(&absent, &absent.configurations().unwrap()).unwrap(),
            episode_session_v1(&present, &present.configurations().unwrap()).unwrap(),
        ];
        let mut rng = SplitMix64::seed(0x40_4040 ^ i as u64);
        for step in 0..MAX_DECISIONS {
            let context = format!("london {london}: decision {step}");
            for s in &sessions {
                assert_eq!(s.current_response(), before.current_response(), "{context}");
                assert_eq!(s.game_state(), before.game_state(), "{context}");
            }
            let FastActorResponseV1::Decision(d) = before.current_response() else {
                break;
            };
            let pick = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
            before.step(d.episode_id, d.step, pick).unwrap();
            for s in &mut sessions {
                s.step(d.episode_id, d.step, pick).unwrap();
            }
        }
    }
}

/// Real complete games, deliberately opt-in (native engine compute), like
/// `collect_episode_v4_real_game_stamps_v4_digests_and_is_rejected_by_a_pure_v3_check`.
/// The absent field's first trajectory has no `deck_format` byte, and the
/// field on the same 60/15 decks collects the same trajectory but for the
/// episode's own field.
#[test]
#[ignore = "root-owned native qualification: two real complete games"]
fn deck_format_absent_keeps_the_first_trajectory_and_present_collects_the_same_game() {
    let collect = |episode: &ExpandedEpisodeV1| {
        let mut policy = FrozenPlayPolicyV1::training_fixture_v4();
        let learner = test_behavior(&policy, false);
        let trajectory = collect_episode(&mut policy, &learner, None, episode).unwrap();
        validate_trajectory(&trajectory).unwrap();
        trajectory
    };
    let lists = [deck("Burn", 60, 15), deck("Burn", 60, 15)];
    let absent = collect(&episode(lists.clone(), false, None));
    let bytes = serde_json::to_vec(&absent).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains("deck_format"));
    let present = collect(&episode(lists, false, LIMITED));
    assert_eq!(present.episode.deck_format, LIMITED);
    let mut present = serde_json::to_value(&present).unwrap();
    present["episode"]
        .as_object_mut()
        .unwrap()
        .remove("deck_format")
        .unwrap();
    assert_eq!(present, serde_json::to_value(&absent).unwrap());
}

/// Two disjoint 40-card decks of Full cube cards (the eternal-fixtures
/// catalog): 16 basics and four copies each of six cube cards per deck.
/// Every named card is on the checked-in cube list, absent from its gap
/// record (so Full in this build) and fully supported.
#[cfg(feature = "eternal-fixtures")]
pub(crate) fn cube_forty_card_lists() -> [ExpandedDeckListV1; 2] {
    const CUBE: &str = include_str!("../../../../data/cube_v1/mtgo_vintage_cube_20261007.json");
    const GAPS: &str =
        include_str!("../../../../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json");
    let cube: serde_json::Value = serde_json::from_str(CUBE).unwrap();
    let gaps: serde_json::Value = serde_json::from_str(GAPS).unwrap();
    let on_cube = |name: &str| {
        cube["cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["name"] == name)
    };
    let gap = |name: &str| {
        gaps.as_object().unwrap().values().any(|value| {
            value
                .as_array()
                .is_some_and(|names| names.iter().any(|n| n == name))
        })
    };
    let id = |name: &str| {
        let id = crate::card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name}"));
        assert!(
            crate::card_def::CARD_DEFS[usize::from(id)].has_full_support(),
            "{name}"
        );
        id
    };
    let list = |label: &str, basic: &str, cards: [&str; 6]| {
        let mut mainboard = vec![id(basic); 16];
        for name in cards {
            assert!(
                on_cube(name) && !gap(name),
                "{name} is not a Full cube card"
            );
            mainboard.extend([id(name); 4]);
        }
        assert_eq!(mainboard.len(), 40);
        ExpandedDeckListV1 {
            label: label.into(),
            mainboard,
            sideboard: Vec::new(),
        }
    };
    [
        list(
            "CubeRed",
            "Mountain",
            [
                "Lightning Bolt",
                "Chain Lightning",
                "Firebolt",
                "Flame Slash",
                "Monastery Swiftspear",
                "Burst Lightning",
            ],
        ),
        list(
            "CubeGreen",
            "Forest",
            [
                "Llanowar Elves",
                "Elvish Mystic",
                "Birds of Paradise",
                "Noble Hierarch",
                "Ignoble Hierarch",
                "Delighted Halfling",
            ],
        ),
    ]
}

/// The design's acceptance case: two 40-card decks of Full cube cards are
/// refused today with "must contain exactly 60 cards" at the collector's
/// check and at the session reset, and reset with the field present, on
/// both resets.
#[cfg(feature = "eternal-fixtures")]
#[test]
fn deck_format_limited40_admits_two_forty_card_cube_decks() {
    let lists = cube_forty_card_lists();
    for london in [false, true] {
        let absent = episode(lists.clone(), london, None);
        let error = absent.configurations().unwrap_err();
        assert!(error.contains("must contain exactly 60 cards"), "{error}");

        let present = episode(lists.clone(), london, LIMITED);
        let configs = present.configurations().unwrap();
        let session = episode_session_v1(&present, &configs).unwrap();
        assert!(
            matches!(session.current_response(), FastActorResponseV1::Decision(_)),
            "london {london}"
        );
        let error = episode_session_v1(&absent, &configs).err().unwrap();
        assert!(
            error.contains("must contain exactly 60 cards, got 40"),
            "{error}"
        );

        let mut short = present.clone();
        for list in short.registered.iter_mut().chain(short.selected.iter_mut()) {
            list.mainboard.pop();
        }
        assert!(short.configurations().unwrap_err().contains("got 39"));
    }
}
