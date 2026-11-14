//! Per-seat priority-window rules on the trainer's episodes
//! (`ExpandedEpisodeV1::priority_windows`, `priority_action_windows_v1`):
//! the absent field keeps every serialized byte and builds the plain session,
//! the field's own refusals, and the session a present field builds, with
//! each physical seat on its own rule. Games here are driven by seeded picks
//! through the collector's own session builder, not a network: they check
//! the windows, not playing strength.

use super::*;
use crate::priority_action_windows_v1::SeatPriorityWindowsV1;
use crate::state::{SplitMix64, Step};
use PauperMulliganModeV1::Learned;
use SeatPriorityWindowsV1::{ActionWindowsEndStepV1, ActionWindowsV1, HarnessV2};

const DECKS: [&str; 9] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries",
];
/// Decisions played per deal.
const MAX_DECISIONS: u64 = 400;

fn deck(id: &str) -> ExpandedDeckListV1 {
    let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: registered.registered_configuration().mainboard().to_vec(),
        sideboard: registered.registered_configuration().sideboard().to_vec(),
    }
}

fn pin(name: &str) -> PinnedFileV1 {
    PinnedFileV1 {
        path: PathBuf::from(name),
        sha256: "ab".repeat(32),
    }
}

fn source() -> ExpandedModelSourceV1 {
    ExpandedModelSourceV1 {
        play_import: pin("import.json"),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: None,
        s_learner: None,
        w_learner: None,
    }
}

fn episode(
    decks: [&str; 2],
    seed: u64,
    starting_player: u8,
    london: bool,
    windows: Option<[SeatPriorityWindowsV1; 2]>,
) -> ExpandedEpisodeV1 {
    ExpandedEpisodeV1 {
        id: format!("priority-windows-{seed}"),
        seed,
        starting_player,
        learner_seat: 0,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: decks.map(deck),
        selected: decks.map(deck),
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
        opening: london.then_some(PauperOpeningV1::LondonV1),
        mulligan_modes: london.then_some([Learned, Learned]),
        priority_windows: windows,
        london_keep_offset: None,
    }
}

fn plain() -> ExpandedEpisodeV1 {
    episode(["Spy", "Burn"], 41, 1, false, None)
}

fn ruled() -> ExpandedEpisodeV1 {
    episode(
        ["Spy", "Burn"],
        41,
        1,
        false,
        Some([ActionWindowsV1, HarnessV2]),
    )
}

/// The session the collector builds for `episode`.
fn session(episode: &ExpandedEpisodeV1) -> FastActorSessionV1 {
    episode_session_v1(episode, &episode.configurations().unwrap()).unwrap()
}

/// The session `episode` built before the field existed: the reset its
/// builder called then, with the builder's own arguments.
fn session_before_the_field(episode: &ExpandedEpisodeV1) -> FastActorSessionV1 {
    let configs = episode.configurations().unwrap();
    let labels = episode.selected.each_ref().map(|d| d.label.clone());
    let mainboards = configs.each_ref().map(|d| d.mainboard().to_vec());
    let starting = PlayerId(episode.starting_player);
    match episode.mulligan_modes {
        Some(modes) => FastActorSessionV1::reset_pauper_london_v1(
            1,
            episode.seed,
            episode.max_physical_decisions,
            episode.max_policy_steps,
            labels,
            mainboards,
            starting,
            modes,
        )
        .unwrap(),
        None => FastActorSessionV1::reset_with_explicit_decks_and_limits_flat_action_v3_environment_v2_with_starting_player_v1(
            1,
            episode.seed,
            episode.max_physical_decisions,
            episode.max_policy_steps,
            labels,
            mainboards,
            starting,
        )
        .unwrap(),
    }
}

#[test]
fn an_absent_field_keeps_every_serialized_byte() {
    let e = plain();
    let value = serde_json::to_value(&e).unwrap();
    assert!(value.get("priority_windows").is_none());
    let bytes = serde_json::to_vec(&e).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("priority_windows"));
    let parsed: ExpandedEpisodeV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed, e);
    assert_eq!(parsed.priority_windows, None);
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);

    // The field writes its rule names by physical seat and reads back
    // exactly; without its key the episode is the plain one, byte for byte.
    let with = ruled();
    let value = serde_json::to_value(&with).unwrap();
    assert_eq!(
        value["priority_windows"],
        serde_json::json!(["action-windows-v1", "harness-v2"])
    );
    assert_eq!(
        serde_json::from_value::<ExpandedEpisodeV1>(value.clone()).unwrap(),
        with
    );
    let mut stripped = value.clone();
    stripped
        .as_object_mut()
        .unwrap()
        .remove("priority_windows")
        .unwrap();
    let stripped: ExpandedEpisodeV1 = serde_json::from_value(stripped).unwrap();
    assert_eq!(stripped, e);
    assert_eq!(serde_json::to_vec(&stripped).unwrap(), bytes);
    let end_step = episode(
        ["Spy", "Burn"],
        41,
        1,
        false,
        Some([HarnessV2, ActionWindowsEndStepV1]),
    );
    let end_step_value = serde_json::to_value(&end_step).unwrap();
    assert_eq!(
        end_step_value["priority_windows"],
        serde_json::json!(["harness-v2", "action-windows-end-step-v1"])
    );
    assert_eq!(
        serde_json::from_value::<ExpandedEpisodeV1>(end_step_value).unwrap(),
        end_step
    );

    // An unknown rule, a rule for one seat only or for three, and a bare
    // rule are refused by the strict reader.
    for bad in [
        serde_json::json!(["action-windows-v2", "harness-v2"]),
        serde_json::json!(["action-windows-v1"]),
        serde_json::json!(["harness-v2", "harness-v2", "harness-v2"]),
        serde_json::json!("action-windows-v1"),
    ] {
        let mut refused = value.clone();
        refused["priority_windows"] = bad.clone();
        assert!(
            serde_json::from_value::<ExpandedEpisodeV1>(refused).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn the_field_is_refused_with_a_reset_start_and_with_a_search_opponent() {
    assert_eq!(plain().priority_windows_v1().unwrap(), [HarnessV2; 2]);
    assert_eq!(
        ruled().priority_windows_v1().unwrap(),
        [ActionWindowsV1, HarnessV2]
    );
    assert!(ruled().configurations().is_ok());
    let configs = plain().configurations().unwrap();
    assert!(episode_session_v1(&ruled(), &configs).is_ok());

    let refused = |edit: &dyn Fn(&mut ExpandedEpisodeV1), needle: &str| {
        let mut e = ruled();
        edit(&mut e);
        let error = e.priority_windows_v1().unwrap_err();
        assert!(error.contains("priority_windows"), "{error}");
        assert!(error.contains(needle), "{error}");
        // With the episode's other field checks, before any game is played.
        let error = e.configurations_checked_v1().unwrap_err();
        assert!(error.contains("priority_windows"), "{error}");
        assert!(error.contains(needle), "{error}");
        // The session builder refuses on its own.
        let error = episode_session_v1(&e, &configs).err().unwrap();
        assert!(error.contains("priority_windows"), "{error}");
        // The refusal is the field's: without it the same episode passes
        // these checks.
        e.priority_windows = None;
        assert_eq!(e.priority_windows_v1().unwrap(), [HarnessV2; 2]);
        assert!(e.configurations_checked_v1().is_ok());
    };
    refused(
        &|e| {
            e.reset = Some(ExpandedResetStartV1 {
                guide: "g".into(),
                game_seed: 1,
                prefix: Vec::new(),
                cut_physical_decision_id: 0,
                cut_observation_sha256: String::new(),
                cut_menu_sha256: String::new(),
                cut_state_hash: String::new(),
            })
        },
        "reset start",
    );
    refused(
        &|e| {
            e.opponent = Some(source());
            e.opponent_search = Some(pin("search.json"));
        },
        "search opponent",
    );
    // Both seats on `harness-v2` is today's surface spelled out, but it is
    // still the field: the same refusals apply.
    let mut spelled_out = ruled();
    spelled_out.priority_windows = Some([HarnessV2; 2]);
    assert!(spelled_out.configurations().is_ok());
    spelled_out.opponent = Some(source());
    spelled_out.opponent_search = Some(pin("search.json"));
    assert!(spelled_out.priority_windows_v1().is_err());
}

/// Without the field the builder's session is the one it built before the
/// field existed, at every decision of the same seeded game, on both resets;
/// no decision of it is a window a rule added. Both seats on `harness-v2`
/// build that session too.
#[test]
fn an_absent_field_builds_the_plain_session_and_plays_its_game() {
    for (i, london) in [false, true].into_iter().enumerate() {
        let decks = [DECKS[5 + i], DECKS[8 - i]];
        let seed = 0x5eed_0d00 + i as u64;
        let absent = episode(decks, seed, i as u8, london, None);
        let spelled_out = episode(decks, seed, i as u8, london, Some([HarnessV2; 2]));
        let mut before = session_before_the_field(&absent);
        let mut sessions = [session(&absent), session(&spelled_out)];
        let mut rng = SplitMix64::seed(seed ^ 0xAB5E);
        for step in 0..MAX_DECISIONS {
            let context = format!("{decks:?} london {london}: decision {step}");
            for s in &sessions {
                assert_eq!(s.current_response(), before.current_response(), "{context}");
                assert_eq!(s.game_state(), before.game_state(), "{context}");
                assert_eq!(
                    s.priority_windows_for_test_v1(),
                    [HarnessV2; 2],
                    "{context}"
                );
                assert_eq!(s.added_priority_window_for_test_v1(), None, "{context}");
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

/// The field's index is the physical seat, on both resets: the session
/// carries exactly the episode's two rules, only a seat on an action-window
/// rule is ever handed a window `HarnessV2` would have passed, the end-step
/// rule adds windows in the end step only, and the action-window rule adds
/// at least one window over the deals.
#[test]
fn a_present_field_gives_each_physical_seat_its_rule() {
    let rules = [
        [ActionWindowsV1, HarnessV2],
        [HarnessV2, ActionWindowsV1],
        [ActionWindowsEndStepV1, ActionWindowsV1],
        [ActionWindowsV1, ActionWindowsEndStepV1],
    ];
    let mut added_by_rule = [0u64; 2];
    let mut deals = 0u64;
    for (i, first) in DECKS.iter().enumerate() {
        for (j, london) in [false, true].into_iter().enumerate() {
            let windows = rules[(i + 2 * j) % rules.len()];
            let second = DECKS[(i + 4 + j) % DECKS.len()];
            let seed = 0x5eed_0e00 + (i * 2 + j) as u64;
            let e = episode(
                [first, second],
                seed,
                ((i + j) % 2) as u8,
                london,
                Some(windows),
            );
            let mut s = session(&e);
            assert_eq!(s.priority_windows_for_test_v1(), windows);
            let mut rng = SplitMix64::seed(seed ^ 0x5EA7);
            for _ in 0..MAX_DECISIONS {
                let FastActorResponseV1::Decision(d) = s.current_response() else {
                    break;
                };
                let context = format!("{first} {second} london {london} {windows:?} seed {seed}");
                if s.added_priority_window_for_test_v1().is_some() {
                    let rule = windows[usize::from(seat(d.acting_player))];
                    match rule {
                        HarnessV2 => {
                            panic!("a harness-v2 seat was handed an added window: {context}")
                        }
                        ActionWindowsV1 => added_by_rule[0] += 1,
                        ActionWindowsEndStepV1 => {
                            assert_eq!(s.game_state().step, Step::End, "{context}");
                            added_by_rule[1] += 1;
                        }
                    }
                }
                let pick = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
                s.step(d.episode_id, d.step, pick).unwrap();
            }
            deals += 1;
        }
    }
    assert_eq!(deals, 18);
    assert!(
        added_by_rule[0] > 0,
        "action-windows-v1 added no window over {deals} deals"
    );
}
