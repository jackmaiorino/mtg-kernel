//! Priority pass offset (`priority_pass_offset_v1`): the episode field and
//! its refusals, the draw at new priority windows over the nine panel decks,
//! record validation (the sampler re-draw and the replayed row marks) and
//! the update batch value. Games here are driven by seeded random logits
//! through the collector's own session and helpers, not a network: they
//! check the windows and the records, not playing strength.

use super::*;
use crate::priority_action_windows_v1::SeatPriorityWindowsV1;
use PauperMulliganModeV1::Learned;
use SeatPriorityWindowsV1::{ActionWindowsEndStepV1, ActionWindowsV1, HarnessV2};

const DECKS: [&str; 9] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries",
];

fn deck(id: &str) -> ExpandedDeckListV1 {
    let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: registered.registered_configuration().mainboard().to_vec(),
        sideboard: registered.registered_configuration().sideboard().to_vec(),
    }
}

fn episode(
    decks: [&str; 2],
    seed: u64,
    starting_player: u8,
    london: bool,
    windows: Option<[SeatPriorityWindowsV1; 2]>,
    offset: Option<f64>,
) -> ExpandedEpisodeV1 {
    ExpandedEpisodeV1 {
        id: format!("pass-offset-{seed}"),
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
        london_keep_offset: None,
        priority_windows: windows,
        priority_pass_offset: offset,
    }
}

fn plain() -> ExpandedEpisodeV1 {
    episode(["Spy", "Burn"], 41, 0, false, None, None)
}

fn windows_episode(offset: f64) -> ExpandedEpisodeV1 {
    episode(
        ["Spy", "Burn"],
        41,
        0,
        false,
        Some([ActionWindowsV1, HarnessV2]),
        Some(offset),
    )
}

fn no_tensor() -> TensorBitsV1 {
    TensorBitsV1 {
        state: Vec::new(),
        object_features: Vec::new(),
        object_card_ids: Vec::new(),
        object_groups: Vec::new(),
        object_node_ids: Vec::new(),
        edge_features: Vec::new(),
        edge_source_indices: Vec::new(),
        edge_target_indices: Vec::new(),
        action_features: Vec::new(),
        action_ref_features: Vec::new(),
        action_ref_card_ids: Vec::new(),
        action_ref_action_indices: Vec::new(),
        action_ref_node_indices: Vec::new(),
        pairing_node_indices: Vec::new(),
    }
}

/// One game of `episode` as the collector plays it, with seeded random raw
/// logits in [-4, 4) in place of a network: each decision is marked from the
/// live session, a marked row is drawn from its offset logits with the
/// unclamped sampler and every other row from the raw logits with the legacy
/// sampler, one draw of the acting seat's stream per decision.
fn play(episode: &ExpandedEpisodeV1) -> (Vec<DecisionRecordV1>, RlSessionTerminalV1) {
    let offset = episode.priority_pass_offset_v1().unwrap();
    let mut session = episode_session_v1(episode, &episode.configurations().unwrap()).unwrap();
    let mut streams = paired_policy_seeds_v1(episode.seed).map(SplitMix64::seed);
    let mut logits_rng = SplitMix64::seed(episode.seed ^ 0x9a55_0ff5);
    let mut legacy = WideCategoricalScratchV1::default();
    let mut unclamped = UnclampedSoftmaxScratchV1::default();
    let mut decisions = Vec::new();
    loop {
        let d = match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => return (decisions, terminal),
            FastActorResponseV1::Decision(d) => d,
        };
        let mark = match offset {
            Some(_) => session.priority_pass_window_v1().unwrap(),
            None => None,
        };
        let width = d.legal_action_count as usize;
        let raw: Vec<f32> = (0..width)
            .map(|_| (logits_rng.next_u64() >> 40) as f32 / 16_777_216.0 * 8.0 - 4.0)
            .collect();
        let draw = streams[usize::from(seat(d.acting_player))].next_u64();
        let selected = match mark.zip(offset) {
            Some((pass_row, offset)) => {
                let sampled =
                    priority_pass_offset_v1::pass_offset_logits_v1(&raw, pass_row, offset).unwrap();
                unclamped.sample(&sampled, draw).unwrap()
            }
            None => legacy.sample(&raw, draw).unwrap(),
        } as u32;
        decisions.push(DecisionRecordV1 {
            step: d.step,
            physical_decision_id: d.physical_decision_id,
            substep_index: d.substep_index,
            substep_count: d.substep_count,
            actor: seat(d.acting_player),
            selected,
            logits: bits(&raw),
            value: 0.0f32.to_bits(),
            tensor: no_tensor(),
            sampler_identity: decision_sampler_identity_v1(width).map(str::to_owned),
            pass_offset_action: mark,
        });
        session.step(d.episode_id, d.step, selected).unwrap();
    }
}

/// The two record checks `validate_trajectory` runs for these rows: the
/// sampler re-draw of every row and, with the field, the replayed marks.
fn validate(
    episode: &ExpandedEpisodeV1,
    decisions: &[DecisionRecordV1],
    terminal: &RlSessionTerminalV1,
) -> Result<(), String> {
    let hashes = episode
        .configurations()?
        .each_ref()
        .map(|c| hex(&c.mainboard_sha256_v1()));
    validate_episode_records_with_learner_sampler_v1(episode, &hashes, decisions, terminal, None)?;
    if episode.priority_pass_offset.is_some() {
        validate_pass_offset_marks_v1(episode, decisions)?;
    }
    Ok(())
}

#[test]
fn an_absent_field_keeps_every_serialized_byte() {
    let e = plain();
    let value = serde_json::to_value(&e).unwrap();
    assert!(value.get("priority_pass_offset").is_none());
    let bytes = serde_json::to_vec(&e).unwrap();
    let parsed: ExpandedEpisodeV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed, e);
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);
    let row = DecisionRecordV1 {
        step: 0,
        physical_decision_id: 0,
        substep_index: 0,
        substep_count: 1,
        actor: 0,
        selected: 0,
        logits: vec![0],
        value: 0,
        tensor: no_tensor(),
        sampler_identity: None,
        pass_offset_action: None,
    };
    let written = serde_json::to_value(&row).unwrap();
    assert!(written.get("pass_offset_action").is_none());
    let marked = DecisionRecordV1 {
        pass_offset_action: Some(2),
        ..row
    };
    let written = serde_json::to_value(&marked).unwrap();
    assert_eq!(written["pass_offset_action"], 2);
    assert_eq!(
        serde_json::from_value::<DecisionRecordV1>(written).unwrap(),
        marked
    );
    // The field writes and reads back exactly.
    let with = windows_episode(6.5);
    let value = serde_json::to_value(&with).unwrap();
    assert_eq!(value["priority_pass_offset"], serde_json::json!(6.5));
    assert_eq!(
        serde_json::from_value::<ExpandedEpisodeV1>(value).unwrap(),
        with
    );
    assert_eq!(with.priority_pass_offset_v1().unwrap(), Some(6.5));
    assert_eq!(e.priority_pass_offset_v1().unwrap(), None);
}

#[test]
fn the_field_is_refused_without_an_action_window_seat_or_with_a_bad_value() {
    // No priority_windows.
    let mut missing = plain();
    missing.priority_pass_offset = Some(6.5);
    assert!(missing.configurations().is_err());
    assert!(missing.priority_pass_offset_v1().is_err());
    // Both seats on HarnessV2: no window would ever be adjusted.
    let mut harness = windows_episode(6.5);
    harness.priority_windows = Some([HarnessV2; 2]);
    assert!(harness.configurations().is_err());
    // Either action-window rule on either seat is accepted.
    for windows in [
        [ActionWindowsV1, HarnessV2],
        [HarnessV2, ActionWindowsEndStepV1],
        [ActionWindowsV1, ActionWindowsV1],
    ] {
        let mut ok = windows_episode(6.5);
        ok.priority_windows = Some(windows);
        assert!(ok.configurations().is_ok(), "{windows:?}");
    }
    for value in [0.0, 1.0e6] {
        assert!(windows_episode(value).configurations().is_ok(), "{value}");
    }
    for bad in [-1.0, -0.0, f64::NAN, f64::INFINITY, 1.0e6 + 1.0, 0.1] {
        assert!(windows_episode(bad).configurations().is_err(), "{bad}");
    }
    // The windows' own refusals: a reset start and a search opponent.
    let mut reset = windows_episode(6.5);
    reset.reset = Some(ExpandedResetStartV1 {
        guide: "guide".into(),
        game_seed: 1,
        prefix: Vec::new(),
        cut_physical_decision_id: 0,
        cut_observation_sha256: "ab".repeat(32),
        cut_menu_sha256: "ab".repeat(32),
        cut_state_hash: "0".repeat(16),
    });
    assert!(reset.priority_pass_offset_v1().is_err());
    let mut search = windows_episode(6.5);
    search.opponent_search = Some(PinnedFileV1 {
        path: "search.json".into(),
        sha256: "ab".repeat(32),
    });
    assert!(search.priority_pass_offset_v1().is_err());
    // Only the expanded collector admits it.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    assert!(windows_episode(6.5)
        .configurations_for_public_collector_v1()
        .is_err());
}

#[test]
fn an_update_batch_carries_one_value() {
    let at = |value| windows_episode(value);
    assert_eq!(batch_priority_pass_offset_v1([]).unwrap(), None);
    assert_eq!(
        batch_priority_pass_offset_v1([&plain(), &plain()]).unwrap(),
        None
    );
    assert_eq!(
        batch_priority_pass_offset_v1([&at(3.0), &at(3.0)]).unwrap(),
        Some(3.0)
    );
    assert_eq!(
        batch_priority_pass_offset_v1([&at(0.0), &at(0.0)]).unwrap(),
        Some(0.0)
    );
    for (a, b) in [
        (at(3.0), at(2.0)),
        (at(3.0), plain()),
        (plain(), at(3.0)),
        (at(0.0), plain()),
        (at(3.0), at(f64::from(f32::from_bits(3.0f32.to_bits() + 1)))),
    ] {
        assert!(batch_priority_pass_offset_v1([&a, &b]).is_err());
    }
    // A malformed value is refused, not compared.
    assert!(batch_priority_pass_offset_v1([&at(-1.0)]).is_err());
}

/// A 1e6 offset passes at every new window of every deal (every deck, both
/// resets, both rules, both seats); every row validates, which proves the
/// unmarked rows, including every window HarnessV2 already offers, were
/// drawn from their raw logits by the legacy sampler, untouched.
#[test]
fn a_large_offset_passes_at_every_new_window_over_the_panel() {
    let mut marked = 0u64;
    let mut deals = 0u64;
    for (i, first) in DECKS.iter().enumerate() {
        for (j, london) in [false, true].into_iter().enumerate() {
            let rules = [
                [ActionWindowsV1, ActionWindowsV1],
                [HarnessV2, ActionWindowsEndStepV1],
            ];
            let windows = rules[(i + j) % 2];
            let second = DECKS[(i + 2 + j) % DECKS.len()];
            let seed = 0x9a55_0000 + (i * 2 + j) as u64;
            let e = episode(
                [first, second],
                seed,
                ((i + j) % 2) as u8,
                london,
                Some(windows),
                Some(1.0e6),
            );
            let (decisions, terminal) = play(&e);
            for row in &decisions {
                if let Some(pass_row) = row.pass_offset_action {
                    marked += 1;
                    assert_eq!(row.selected, pass_row, "{first} {second} seed {seed}");
                    assert!(!windows[usize::from(row.actor)].is_harness_v2());
                }
            }
            validate(&e, &decisions, &terminal)
                .unwrap_or_else(|error| panic!("{first} {second} seed {seed}: {error}"));
            deals += 1;
        }
    }
    assert_eq!(deals, 18);
    assert!(marked > 0, "no new window over {deals} deals");
}

/// Record validation trusts no mark: a missing, extra or wrong mark fails
/// the replay, and a mark without the episode field fails row validation.
#[test]
fn validation_rejects_a_tampered_mark() {
    let e = episode(
        ["Elves", "Faeries"],
        0x9a55_1000,
        0,
        false,
        Some([ActionWindowsV1, ActionWindowsV1]),
        Some(2.0),
    );
    let (decisions, terminal) = play(&e);
    validate(&e, &decisions, &terminal).unwrap();
    validate_pass_offset_marks_v1(&e, &decisions).unwrap();
    let marked = decisions
        .iter()
        .position(|row| row.pass_offset_action.is_some())
        .expect("a new window in the game");
    let unmarked = decisions
        .iter()
        .position(|row| row.pass_offset_action.is_none() && row.logits.len() > 1)
        .expect("an unmarked decision with a choice");
    let tamper = |index: usize, mark: Option<u32>| {
        let mut copy = decisions.clone();
        copy[index].pass_offset_action = mark;
        copy
    };
    let pass_row = decisions[marked].pass_offset_action.unwrap();
    let width = decisions[marked].logits.len() as u32;
    let mut cases = vec![
        ("missing", tamper(marked, None)),
        ("extra", tamper(unmarked, Some(0))),
    ];
    if width > 1 {
        cases.push(("wrong row", tamper(marked, Some((pass_row + 1) % width))));
    }
    for (label, copy) in &cases {
        assert!(
            validate_pass_offset_marks_v1(&e, copy).is_err(),
            "{label} mark accepted by the replay"
        );
        assert!(validate(&e, copy, &terminal).is_err(), "{label}");
    }
    // The same rows under an episode without the field.
    let mut without = e.clone();
    without.priority_pass_offset = None;
    let error = validate(&without, &decisions, &terminal).unwrap_err();
    assert!(error.contains("without priority_pass_offset"), "{error}");
    // And an unmarked game of that episode validates as before.
    let (plain_rows, plain_terminal) = play(&without);
    assert!(plain_rows
        .iter()
        .all(|row| row.pass_offset_action.is_none()));
    validate(&without, &plain_rows, &plain_terminal).unwrap();
}

/// Real collection through the fixture policy: every new window passes under
/// a 1e6 offset, the stored logits stay raw, the trajectory validates, and a
/// tampered mark is refused by `validate_trajectory`.
#[test]
#[ignore = "root-owned native qualification: real complete games"]
fn collection_marks_and_passes_new_windows_and_validates() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    let learner = test_behavior(&policy, false);
    let mut marked = 0;
    for (i, pair) in [["Spy", "Burn"], ["Elves", "Faeries"]]
        .into_iter()
        .enumerate()
    {
        let e = episode(
            pair,
            0x9a55_2000 + i as u64,
            (i % 2) as u8,
            i == 1,
            Some([ActionWindowsV1, ActionWindowsEndStepV1]),
            Some(1.0e6),
        );
        let trajectory = collect_episode(&mut policy, &learner, None, &e).unwrap();
        validate_trajectory(&trajectory).unwrap();
        for row in &trajectory.decisions {
            if let Some(pass_row) = row.pass_offset_action {
                marked += 1;
                assert_eq!(row.selected, pass_row);
                assert!(f32::from_bits(row.logits[pass_row as usize]) < 1.0e3);
            }
        }
        if let Some(index) = trajectory
            .decisions
            .iter()
            .position(|row| row.pass_offset_action.is_some())
        {
            let mut tampered = trajectory.clone();
            tampered.decisions[index].pass_offset_action = None;
            assert!(validate_trajectory(&tampered).is_err());
        }
    }
    assert!(marked > 0);
}
