//! Pinned training goldens for one 60-game London update (issue 921).
//!
//! An engine rules change (Undercity completes only once Throne of the Dead
//! Three's room ability leaves the stack) changed the after-state of 60-game
//! London updates on the nine-deck Pauper base without failing any golden:
//! none covered a London update of that size, with prepared workers, or a
//! game that reaches the changed dungeon state. These two tests pin that
//! update, so the next change to engine rules, encoding or update arithmetic
//! that reaches such games fails here instead of only in a trainer
//! comparison.
//!
//! Each test collects 60 episodes from a fixed seed with a fresh V4
//! descriptor transferred to FlatV7 (London requires V7), London mulligans
//! learned on both seats, over the nine checked-in Pauper decks (Elves and
//! Spy, the decks holding the pool's initiative card, included), then runs
//! one prepared update at 12 preparation workers and compares its
//! `after_state_sha256` with a pinned constant:
//!
//! - `flag_free_london_60_game_prepared_update_state_is_pinned`: no optional
//!   episode or loss field (GAE with the default pregame advantage).
//! - `london_60_game_prepared_update_with_windows_and_terminal_pregame_state_is_pinned`:
//!   the same with `priority_windows` set to `action-windows-v1` on both
//!   seats and `pregame_advantage: terminal_v1` on the loss.
//!
//! The collection runs on 12 workers to keep the test short; trajectories
//! join in schedule order, so the worker count does not change the batch.
//! One update is enough: the issue's difference appears from the first one.
//!
//! The state hashes differ between MSVC and Linux GNU libm in the last bits
//! (as for the other pinned trainer states in this module), so each test has
//! one pin per system. A pin still set to `PLACEHOLDER` fails with
//! `expected PLACEHOLDER, computed <hex>`, which gives the value to pin.
//!
//! Unlike the other real-game training qualifications in this module, both
//! tests run in CI (issue 921): a change to training results on these recipes
//! must fail a check. Run them alone with
//! `cargo test --release --locked -p mtg-kernel --lib london_update_golden_tests`.

use super::*;
use crate::priority_action_windows_v1::SeatPriorityWindowsV1;
use PauperMulliganModeV1::Learned;

/// The nine-deck Pauper base.
const DECKS: [&str; 9] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries",
];
const EPISODES: usize = 60;
/// Episode `index` plays seed `SEED_BASE + index`.
const SEED_BASE: u64 = 921;
const COLLECTION_WORKERS: usize = 12;
const PREPARATION_WORKERS: usize = 12;
/// Above the historical 256 MiB, which holds about ten games; the bound only
/// refuses an oversized batch and does not change the update.
const MAX_PREPARED_TENSOR_MEBIBYTES: usize = 2048;

#[cfg(target_os = "windows")]
const FLAG_FREE_LONDON_60_STATE_SHA256_V1: &str =
    "c998faaa0ca6ccf5474eff84f4d4eaac28ad91c7ea286a0bf59092f52003dd04";
#[cfg(not(target_os = "windows"))]
const FLAG_FREE_LONDON_60_STATE_SHA256_V1: &str =
    "ef46087a88936a76b2e2cfbac168c7dd8bae801a03e4c8de36ac848fbf032cbe";
#[cfg(target_os = "windows")]
const WINDOWS_TERMINAL_LONDON_60_STATE_SHA256_V1: &str =
    "821650a659fc7b12f97bf4dce5e234837ed9eea3aa8b50538727ae8cd410a4ee";
#[cfg(not(target_os = "windows"))]
const WINDOWS_TERMINAL_LONDON_60_STATE_SHA256_V1: &str =
    "b5cf227cfeabf4fee63d3d4bfdc7076b8b5bdd5121a9f9072cea5a64d05c7497";

fn deck(id: &str) -> ExpandedDeckListV1 {
    let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: registered.registered_configuration().mainboard().to_vec(),
        sideboard: registered.registered_configuration().sideboard().to_vec(),
    }
}

/// The schedule's pair for episode `index`: each deck in turn in seat 0,
/// against the next deck at a distance that grows by one every nine
/// episodes, so the two seats never hold the same deck and every deck plays
/// in both seats.
fn pairing(index: usize) -> [&'static str; 2] {
    let first = index % DECKS.len();
    let distance = 1 + (index / DECKS.len()) % (DECKS.len() - 1);
    [DECKS[first], DECKS[(first + distance) % DECKS.len()]]
}

/// Episode `index`: London, both seats learned, the learner seat alternating
/// every episode and the starting player every two, so every seat and
/// start combination occurs.
fn episode(
    index: usize,
    priority_windows: Option<[SeatPriorityWindowsV1; 2]>,
) -> ExpandedEpisodeV1 {
    let decks = pairing(index);
    ExpandedEpisodeV1 {
        id: format!("london-60-{index}"),
        seed: SEED_BASE + index as u64,
        starting_player: ((index / 2) % 2) as u8,
        learner_seat: (index % 2) as u8,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: decks.map(deck),
        selected: decks.map(deck),
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        max_stack_objects: None,
        runtime_rules: None,
        reset: None,
        opening: Some(PauperOpeningV1::LondonV1),
        mulligan_modes: Some([Learned, Learned]),
        priority_windows,
        london_keep_offset: None,
    }
}

fn gae(pregame_advantage: PregameAdvantageV1) -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.95,
        entropy_coefficient: 0.0,
        pregame_advantage,
    }
}

/// Collects the 60 episodes from a fresh V4 descriptor transferred to FlatV7
/// and runs one prepared update on them; returns the update's receipt.
fn london_60_game_update_receipt_v1(
    label: &str,
    priority_windows: Option<[SeatPriorityWindowsV1; 2]>,
    loss_selection: ExpandedLossSelectionV1,
) -> Value {
    let root = crate::test_temp_dir_v1::TempTestDir::new_v1(&format!("london-60-{label}"));
    let descriptor = fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4,
        |_parameters| {},
    );
    let descriptor_path = root.join("descriptor.json");
    let descriptor_bytes = serde_json::to_vec(&descriptor).unwrap();
    fs::write(&descriptor_path, &descriptor_bytes).unwrap();
    let v7 = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V7;
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: descriptor_path.canonicalize().unwrap(),
            sha256: sha(&descriptor_bytes),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: v7.feature_contract_digest.into(),
            expected_feature_encoding_digest: v7.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
        w_learner: None,
    };
    let episodes: Vec<_> = (0..EPISODES)
        .map(|index| episode(index, priority_windows))
        .collect();
    let collected = execute_v1(ExpandedTrainingCommandV1::CollectParallel {
        source: source.clone(),
        episodes,
        workers: COLLECTION_WORKERS,
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        ex1: None,
        record_envelope: RecordEnvelopeV1::default(),
        trajectory_format: crate::expanded_deck_training_v1::TrajectoryRecordFormatV1::Json,
        output_directory: root.join("collect"),
    })
    .unwrap_or_else(|error| panic!("{label}: collection failed: {error}"));
    let trajectories: Vec<PinnedFileV1> =
        serde_json::from_value(collected["trajectories"].clone()).unwrap();
    assert_eq!(trajectories.len(), EPISODES, "{label}");
    let receipt = execute_v1(ExpandedTrainingCommandV1::UpdatePrepared {
        source,
        card_semantics: None,
        trajectories,
        learning_rate: 0.0003,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection,
        preparation_workers: PREPARATION_WORKERS,
        max_prepared_tensor_mebibytes: MAX_PREPARED_TENSOR_MEBIBYTES,
        max_trajectory_batch_mebibytes: DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::default(),
        record_envelope: RecordEnvelopeV1::default(),
        output_directory: root.join("update"),
    })
    .unwrap_or_else(|error| panic!("{label}: update failed: {error}"));
    assert_eq!(
        receipt["update_preparation"]["requested_workers"], PREPARATION_WORKERS,
        "{label}"
    );
    receipt
}

/// The receipt's after-state hash, which must equal `pinned`. A pin still
/// set to the placeholder fails with the computed value.
fn assert_after_state_pinned_v1(label: &str, receipt: &Value, pinned: &str) {
    let computed = receipt["after_state_sha256"]
        .as_str()
        .unwrap_or_else(|| panic!("{label}: the receipt has no after_state_sha256"));
    assert!(
        computed == pinned,
        "{label}: expected {pinned}, computed {computed}"
    );
}

#[test]
fn flag_free_london_60_game_prepared_update_state_is_pinned() {
    let label = "flag-free";
    let receipt = london_60_game_update_receipt_v1(label, None, gae(PregameAdvantageV1::Gae));
    assert!(receipt.get("pregame_advantage").is_none(), "{label}");
    assert_after_state_pinned_v1(label, &receipt, FLAG_FREE_LONDON_60_STATE_SHA256_V1);
}

#[test]
fn london_60_game_prepared_update_with_windows_and_terminal_pregame_state_is_pinned() {
    use SeatPriorityWindowsV1::ActionWindowsV1;
    let label = "windows-terminal";
    let receipt = london_60_game_update_receipt_v1(
        label,
        Some([ActionWindowsV1, ActionWindowsV1]),
        gae(PregameAdvantageV1::TerminalV1),
    );
    assert_eq!(
        receipt["pregame_advantage"]["mode"], "terminal_v1",
        "{label}"
    );
    assert_after_state_pinned_v1(label, &receipt, WINDOWS_TERMINAL_LONDON_60_STATE_SHA256_V1);
}
