//! The trajectory batch bound of a prepared update
//! (`max_trajectory_batch_mebibytes`) through the real Collect and
//! UpdatePrepared commands: a batch just over the historical 512 MiB is
//! refused with the historical error when the key is absent (or 512), and
//! accepted, reaching the same state as the same games read plainly, when the
//! key allows it. The same for the S learner, its verified prepared replay
//! command and the wide learner, and for the physical-group bound that
//! follows the key, through every prepared update that plans groups. The run
//! driver's own forwarding, to `Ex1Update` and to its recovery checks, is
//! tested with the driver (`native_expanded_training_run_v1`).

use super::*;
use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;
use crate::test_temp_dir_v1::TempTestDir;

fn list(id: &str) -> ExpandedDeckListV1 {
    let d = checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let c = d.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: c.mainboard().to_vec(),
        sideboard: c.sideboard().to_vec(),
    }
}

fn gae() -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
    }
}

fn prepared(
    source: &ExpandedModelSourceV1,
    trajectories: &[PinnedFileV1],
    max_trajectory_batch_mebibytes: usize,
    output_directory: PathBuf,
) -> ExpandedTrainingCommandV1 {
    ExpandedTrainingCommandV1::UpdatePrepared {
        source: source.clone(),
        trajectories: trajectories.to_vec(),
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        preparation_workers: 2,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        max_trajectory_batch_mebibytes,
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::default(),
        record_envelope: RecordEnvelopeV1::None,
        output_directory,
    }
}

/// A synthetic fresh source and three real games collected from it.
fn fixture(tag: &str) -> (TempTestDir, ExpandedModelSourceV1, Vec<PinnedFileV1>) {
    let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
    let root = TempTestDir::new_v1(tag);
    let descriptor = fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        feature_identity,
        |_parameters| {},
    );
    let descriptor_path = root.join("descriptor.json");
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    fs::write(&descriptor_path, &bytes).unwrap();
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: descriptor_path.canonicalize().unwrap(),
            sha256: hex(&Sha256::digest(bytes)),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
        w_learner: None,
    };
    let decks = [list("Affinity"), list("Terror")];
    let episodes = (0..3u64)
        .map(|index| ExpandedEpisodeV1 {
            id: format!("e-batch-bound-{index}"),
            seed: 2026111900 + index,
            starting_player: (index % 2) as u8,
            learner_seat: 0,
            opponent: Some(source.clone()),
            opponent_search: None,
            opponent_kind: None,
            registered: decks.clone(),
            selected: decks.clone(),
            postboard: false,
            max_physical_decisions: 100000,
            max_policy_steps: 1000000,
            runtime_rules: None,
            reset: None,
            opening: None,
            mulligan_modes: None,
            priority_windows: None,
            london_keep_offset: None,
        })
        .collect();
    let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
        source: source.clone(),
        episodes,
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        ex1: None,
        output_directory: root.join("collect"),
    })
    .unwrap();
    let trajectories = serde_json::from_value(collection["trajectories"].clone()).unwrap();
    (root, source, trajectories)
}

/// The same games as sealed files whose record bytes (each trajectory
/// followed by spaces, which parse to the same trajectory) total exactly one
/// byte over the historical batch bound.
fn padded_just_over_the_default_bound(
    root: &Path,
    trajectories: &[PinnedFileV1],
) -> Vec<PinnedFileV1> {
    let originals: Vec<Vec<u8>> = trajectories
        .iter()
        .map(|pin| fs::read(&pin.path).unwrap())
        .collect();
    let target = MAX_BATCH_BYTES + 1;
    let plain: u64 = originals.iter().map(|bytes| bytes.len() as u64).sum();
    let count = originals.len() as u64;
    let share = (target - plain) / count;
    let directory = root.join("padded");
    fs::create_dir_all(&directory).unwrap();
    let mut total = 0u64;
    let pins = originals
        .into_iter()
        .enumerate()
        .map(|(index, mut bytes)| {
            let pad = if index as u64 == count - 1 {
                target - plain - share * (count - 1)
            } else {
                share
            };
            bytes.resize(bytes.len() + pad as usize, b' ');
            total += bytes.len() as u64;
            let sealed = RecordEnvelopeV1::Zstd1.seal_owned_v1(bytes).unwrap();
            let path = directory.join(format!("episode-{index:04}.json"));
            fs::write(&path, &sealed).unwrap();
            PinnedFileV1 {
                path,
                sha256: sha(&sealed),
            }
        })
        .collect();
    assert_eq!(total, MAX_BATCH_BYTES + 1);
    pins
}

/// One unsealed file of 1 MiB and a byte under a pin that is not its hash.
/// An update whose size check before any read takes the configured bound of
/// 1 MiB refuses it by size; under the historical 512 MiB that check passes
/// and the pinned read then refuses the file by its hash, a different error.
fn unsealed_over_1_mib_with_bogus_hash(root: &Path, name: &str) -> Vec<PinnedFileV1> {
    let path = root.join(name);
    fs::write(&path, vec![b' '; (1 << 20) + 1]).unwrap();
    vec![PinnedFileV1 {
        path,
        sha256: "0".repeat(64),
    }]
}

/// The physical-group bound of every update up to the default batch bound.
const HISTORICAL_MAX_GROUPS: usize = 65_536;

/// A row's tensors with nothing in them.
fn empty_tensor_bits() -> TensorBitsV1 {
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

/// `net8` with a wide learner whose lineage starts at the runner's fixed wide
/// net: it loads as a wide learner and reaches the update's checks. No game
/// is played with it.
fn fixed_wide_source(root: &Path, net8: &ExpandedModelSourceV1) -> ExpandedModelSourceV1 {
    let state = crate::native_policy_train_step_v1::NativePolicyValueTrainStateWideV1::new_wide_v1(
        crate::native_policy_value_net_v1::NativePolicyValueNetWideV1::runner_fixed_wide_v1(
            crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1::contract_wide_v1(),
        )
        .unwrap(),
    )
    .unwrap();
    let bytes = w_learner_v1::encode_w_state_v1(&state).unwrap();
    let path = root.join("wide-initial.bin");
    fs::write(&path, &bytes).unwrap();
    ExpandedModelSourceV1 {
        w_learner: Some(Box::new(w_learner_v1::WLearnerSourceV1 {
            schema: w_learner_v1::W_LEARNER_SOURCE_SCHEMA_V1.into(),
            initial: PinnedFileV1 {
                path,
                sha256: sha(&bytes),
            },
        })),
        ..net8.clone()
    }
}

/// Sealed synthetic self-play trajectories of `source`'s learner, one file
/// for each entry of `group_counts` with that many physical decisions, each
/// of one substep and all the learner's. Every row is `row` with its own
/// step: a single action, which every sampler draw selects. The records carry
/// what the collector stamps for this source, so they pass the update's input
/// checks; whether a row replays is up to `row`.
fn sealed_synthetic_group_batch(
    directory: &Path,
    source: &ExpandedModelSourceV1,
    group_counts: &[usize],
    row: &DecisionRecordV1,
) -> Vec<PinnedFileV1> {
    assert_eq!(row.logits.len(), 1);
    fs::create_dir_all(directory).unwrap();
    let (policy, identity) = load_expanded_inference_any_math_v1(source).unwrap();
    group_counts
        .iter()
        .enumerate()
        .map(|(episode_index, &count)| {
            let episode = s_update_fixture_v1::episode_v1(
                &format!("groups-{episode_index}"),
                2_026_111_950 + episode_index as u64,
                None,
            )
            .unwrap();
            let configs = episode.configurations().unwrap();
            let decisions: Vec<DecisionRecordV1> = (0..count as u64)
                .map(|step| {
                    let mut decision = row.clone();
                    decision.step = step;
                    decision.physical_decision_id = step;
                    decision.substep_index = 0;
                    decision.substep_count = 1;
                    decision.actor = episode.learner_seat;
                    decision.selected = 0;
                    decision
                })
                .collect();
            let trajectory = ExpandedTrajectoryV1 {
                schema: trajectory_schema_v1(policy.identity_v1(), None).into(),
                feature_contract_digest: no_state_digest_v1::effective_contract_digest_v1(&policy),
                feature_encoding_digest: policy
                    .feature_identity_v1()
                    .feature_encoding_digest
                    .into(),
                card_db_hash: format!("{KERNEL_CARDDB_HASH:016x}"),
                source_import: policy.identity_v1().clone(),
                behavior_state_sha256: identity.state_sha256.clone(),
                seat_behaviors: None,
                configuration_sha256: configs.each_ref().map(|c| hex(&c.mainboard_sha256_v1())),
                terminal: RlSessionTerminalV1 {
                    schema_version: RL_SESSION_SCHEMA_VERSION,
                    deck_ids: episode.selected.each_ref().map(|d| d.label.clone()),
                    deck_hashes: configs
                        .each_ref()
                        .map(|c| explicit_deck_hash_v1(c.mainboard())),
                    episode_id: 1,
                    terminal_outcome: crate::rl::TerminalOutcomeV1::P0Win,
                    terminal_classification: TerminalClassificationV1::Natural,
                    terminal_code: TerminalSafeCodeV2::NaturalGameOver,
                    winner: Some(PlayerSeatV1::P0),
                    terminal_reward: [1, -1],
                    terminal_reason: "synthetic physical-group bound fixture".into(),
                    policy_step_count: count as u64,
                    physical_decision_count: count as u64,
                },
                episode,
                decisions,
                learner_sampler: None,
                training_math: policy.training_math_v1(),
                input_contract: None,
                opening_record: None,
                search_played: None,
            };
            let sealed = RecordEnvelopeV1::Zstd1
                .seal_owned_v1(serde_json::to_vec(&trajectory).unwrap())
                .unwrap();
            let path = directory.join(format!("episode-{episode_index:04}.json"));
            fs::write(&path, &sealed).unwrap();
            PinnedFileV1 {
                path,
                sha256: sha(&sealed),
            }
        })
        .collect()
}

#[test]
fn an_update_just_over_the_default_batch_bound_runs_when_the_key_allows_it() {
    let (root, source, trajectories) = fixture("trajectory-batch-bound");
    let plain = execute_v1(prepared(
        &source,
        &trajectories,
        DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
        root.join("plain"),
    ))
    .unwrap();
    let padded = padded_just_over_the_default_bound(&root, &trajectories);
    // Absent (the default) and an explicit 512 refuse it with the historical
    // error, before any file of the batch is opened.
    for (mebibytes, name) in [
        (DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES, "default"),
        (512, "explicit"),
    ] {
        assert_eq!(
            execute_v1(prepared(&source, &padded, mebibytes, root.join(name))).unwrap_err(),
            "trajectory batch exceeds 512 MiB CPU ingestion bound"
        );
    }
    // One MiB more admits it, and the update reaches the plain update's state.
    let raised = execute_v1(prepared(&source, &padded, 513, root.join("raised"))).unwrap();
    assert_eq!(raised["complete"], true);
    assert_eq!(raised["after_state_sha256"], plain["after_state_sha256"]);
    assert_eq!(raised["before_state_sha256"], plain["before_state_sha256"]);
    // A bound under the batch names itself.
    assert_eq!(
        execute_v1(prepared(&source, &padded, 1, root.join("small"))).unwrap_err(),
        "trajectory batch exceeds 1 MiB CPU ingestion bound"
    );
    // The update's size check before any read takes the configured bound too.
    let unsealed = unsealed_over_1_mib_with_bogus_hash(&root, "net8-unsealed.json");
    assert_eq!(
        execute_v1(prepared(&source, &unsealed, 1, root.join("meta-small"))).unwrap_err(),
        "trajectory batch exceeds 1 MiB CPU ingestion bound"
    );
    // Out-of-range values are refused before any work.
    for refused in [0, 4097] {
        assert_eq!(
            execute_v1(prepared(
                &source,
                &trajectories,
                refused,
                root.join("refused")
            ))
            .unwrap_err(),
            "max_trajectory_batch_mebibytes must be in 1..=4096"
        );
    }
    // One preparation worker keeps the default, as on the run config; with
    // the default it still runs.
    let one_worker = |mebibytes, name: &str| match prepared(
        &source,
        &trajectories,
        mebibytes,
        root.join(name),
    ) {
        ExpandedTrainingCommandV1::UpdatePrepared {
            preparation_workers: _,
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            loss_selection,
            max_prepared_tensor_mebibytes,
            max_trajectory_batch_mebibytes,
            line_b,
            training_math,
            record_format,
            record_envelope,
            output_directory,
        } => execute_v1(ExpandedTrainingCommandV1::UpdatePrepared {
            preparation_workers: 1,
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            loss_selection,
            max_prepared_tensor_mebibytes,
            max_trajectory_batch_mebibytes,
            line_b,
            training_math,
            record_format,
            record_envelope,
            output_directory,
        }),
        _ => unreachable!(),
    };
    assert_eq!(
        one_worker(2048, "one-worker-raised").unwrap_err(),
        "max_trajectory_batch_mebibytes applies to prepared updates only (preparation_workers > 1)"
    );
    let serial = one_worker(DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES, "one-worker").unwrap();
    assert_eq!(serial["after_state_sha256"], plain["after_state_sha256"]);
}

/// The S learner update honors `max_trajectory_batch_mebibytes` as the net-8
/// prepared update does: refused at default (512) with the historical error,
/// admitted at 513 reaching the plain update's exact state, and refused with
/// the configured bound when lower. The verified prepared replay command
/// (`UpdateSVerifiedPrepared`) takes the bound the same way on the same games,
/// through its verified learner and without falling back. The games are the
/// saved-batch fixture's initial batch (`s_update_fixture_v1`): the fresh S
/// against itself and against net-8.
#[test]
fn s_learner_prepared_update_honors_max_trajectory_batch_mebibytes() {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let root = TempTestDir::new_v1("s-trajectory-batch-bound");
            let net8 = s_update_fixture_v1::net8_source_v1(&root).unwrap();
            let s = s_update_fixture_v1::s_source_v1(
                &root,
                &net8,
                s_update_fixture_v1::TEST_CONFIG_V1,
                s_update_fixture_v1::S_INIT_SEED_V1,
            )
            .unwrap();
            let seed = s_update_fixture_v1::DEFAULT_SEED_V1;
            let episodes = vec![
                s_update_fixture_v1::episode_v1("s-batch-bound-0", seed, Some(s.clone())).unwrap(),
                s_update_fixture_v1::episode_v1("s-batch-bound-1", seed + 1, Some(net8.clone()))
                    .unwrap(),
            ];
            let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
                source: s.clone(),
                episodes,
                max_non_natural_episode_fraction: 0.0,
                collection_sampler: CollectionSamplerV1::Legacy,
                training_math: TrainingMathV1::PlatformLibm,
                ex1: None,
                output_directory: root.join("collect"),
            })
            .unwrap();
            let trajectories: Vec<PinnedFileV1> =
                serde_json::from_value(collection["trajectories"].clone()).unwrap();
            let plain = execute_v1(prepared(
                &s,
                &trajectories,
                DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
                root.join("plain"),
            ))
            .unwrap();
            let padded = padded_just_over_the_default_bound(&root, &trajectories);
            assert_eq!(
                execute_v1(prepared(
                    &s,
                    &padded,
                    DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
                    root.join("default"),
                ))
                .unwrap_err(),
                "trajectory batch exceeds 512 MiB CPU ingestion bound"
            );
            let raised = execute_v1(prepared(&s, &padded, 513, root.join("raised"))).unwrap();
            assert_eq!(raised["complete"], true);
            assert_eq!(raised["after_state_sha256"], plain["after_state_sha256"]);
            assert_eq!(raised["before_state_sha256"], plain["before_state_sha256"]);
            assert_eq!(
                execute_v1(prepared(&s, &padded, 1, root.join("small"))).unwrap_err(),
                "trajectory batch exceeds 1 MiB CPU ingestion bound"
            );
            // The S update's size check before any read takes the configured
            // bound too.
            let unsealed = unsealed_over_1_mib_with_bogus_hash(&root, "s-unsealed.json");
            assert_eq!(
                execute_v1(prepared(&s, &unsealed, 1, root.join("meta-small"))).unwrap_err(),
                "trajectory batch exceeds 1 MiB CPU ingestion bound"
            );

            // The verified prepared replay command, on the same batches.
            let verified = |trajectories: &[PinnedFileV1], mebibytes: usize, name: &str| {
                execute_v1(ExpandedTrainingCommandV1::UpdateSVerifiedPrepared {
                    source: s.clone(),
                    trajectories: trajectories.to_vec(),
                    learning_rate: 0.0001,
                    value_coefficient: 0.5,
                    loss_selection: gae(),
                    preparation_workers: 2,
                    max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
                    max_trajectory_batch_mebibytes: mebibytes,
                    tape_cap_bytes: 16 << 20,
                    output_directory: root.join(name),
                })
            };
            let verified_plain = verified(
                &trajectories,
                DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
                "verified-plain",
            )
            .unwrap();
            assert_eq!(
                verified_plain["verified_replay"]["used"], true,
                "{verified_plain}"
            );
            assert_eq!(
                verified(
                    &padded,
                    DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
                    "verified-default"
                )
                .unwrap_err(),
                "trajectory batch exceeds 512 MiB CPU ingestion bound"
            );
            let verified_raised = verified(&padded, 513, "verified-raised").unwrap();
            assert_eq!(verified_raised["complete"], true);
            assert_eq!(
                verified_raised["verified_replay"]["used"], true,
                "{verified_raised}"
            );
            assert_eq!(
                verified_raised["after_state_sha256"],
                verified_plain["after_state_sha256"]
            );
            assert_eq!(
                verified_raised["before_state_sha256"],
                verified_plain["before_state_sha256"]
            );
            for (pins, name) in [
                (&padded, "verified-small"),
                (&unsealed, "verified-meta-small"),
            ] {
                assert_eq!(
                    verified(pins, 1, name).unwrap_err(),
                    "trajectory batch exceeds 1 MiB CPU ingestion bound"
                );
            }
        })
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

/// The wide learner (`w_learner_v1`) honors `max_trajectory_batch_mebibytes`
/// as net-8 and S do: refused at default (512) with the historical error,
/// admitted at 513 reaching the plain update's exact state, refused with the
/// configured bound when lower, and by its size check before any read. The
/// learner and its games are `w_learner_v1`'s own end-to-end test's: a net-8
/// checkpoint widened, playing itself and net-8.
#[test]
fn w_learner_prepared_update_honors_max_trajectory_batch_mebibytes() {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let root = TempTestDir::new_v1("w-trajectory-batch-bound");
            let net8 = s_update_fixture_v1::net8_source_v1(&root).unwrap();
            let episode = |id: &str, seed: u64, opponent: Option<ExpandedModelSourceV1>| {
                s_update_fixture_v1::episode_v1(id, seed, opponent).unwrap()
            };
            let collect = |source: &ExpandedModelSourceV1,
                           episodes: Vec<ExpandedEpisodeV1>,
                           name: &str|
             -> Vec<PinnedFileV1> {
                let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
                    source: source.clone(),
                    episodes,
                    max_non_natural_episode_fraction: 0.0,
                    collection_sampler: CollectionSamplerV1::Legacy,
                    training_math: TrainingMathV1::PlatformLibm,
                    ex1: None,
                    output_directory: root.join(name),
                })
                .unwrap();
                serde_json::from_value(collection["trajectories"].clone()).unwrap()
            };
            // A net-8 checkpoint: the fresh shell after one update of its own
            // two games.
            let parent_games = collect(
                &net8,
                vec![
                    episode("n8-self", 2_026_111_301, None),
                    episode("n8-self-b", 2_026_111_302, None),
                ],
                "n8-collect",
            );
            let parent_update = execute_v1(ExpandedTrainingCommandV1::Update {
                source: net8.clone(),
                trajectories: parent_games,
                learning_rate: 1.0e-3,
                value_coefficient: 0.5,
                update_backend: ExpandedUpdateBackendV1::Cpu,
                update_backward_execution: UpdateBackwardExecutionV1::Sequential,
                loss_selection: gae(),
                line_b: None,
                training_math: TrainingMathV1::PlatformLibm,
                record_format: CheckpointRecordFormatV1::V1,
                output_directory: root.join("n8-update"),
            })
            .unwrap();
            let parent = ExpandedModelSourceV1 {
                checkpoint: Some(
                    serde_json::from_value(parent_update["checkpoint"].clone()).unwrap(),
                ),
                ..net8.clone()
            };
            let widened = w_learner_v1::widen_source_v1(
                &parent,
                "test|parent|1",
                w_learner_v1::WideningOptimizerV1::Keep,
                &root.join("widen"),
            )
            .unwrap();
            let wide: ExpandedModelSourceV1 =
                serde_json::from_value(widened["source"].clone()).unwrap();
            assert!(wide.w_learner.is_some());
            let trajectories = collect(
                &wide,
                vec![
                    episode("w-self", 2_026_111_311, None),
                    episode("w-vs-net8", 2_026_111_312, Some(net8.clone())),
                ],
                "w-collect",
            );
            let plain = execute_v1(prepared(
                &wide,
                &trajectories,
                DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
                root.join("plain"),
            ))
            .unwrap();
            assert_eq!(plain["complete"], true);
            assert!(plain.get("w_learner").is_some());
            let padded = padded_just_over_the_default_bound(&root, &trajectories);
            assert_eq!(
                execute_v1(prepared(
                    &wide,
                    &padded,
                    DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
                    root.join("default"),
                ))
                .unwrap_err(),
                "trajectory batch exceeds 512 MiB CPU ingestion bound"
            );
            let raised = execute_v1(prepared(&wide, &padded, 513, root.join("raised"))).unwrap();
            assert_eq!(raised["complete"], true);
            assert!(raised.get("w_learner").is_some());
            assert_eq!(raised["after_state_sha256"], plain["after_state_sha256"]);
            assert_eq!(raised["before_state_sha256"], plain["before_state_sha256"]);
            assert_eq!(
                execute_v1(prepared(&wide, &padded, 1, root.join("small"))).unwrap_err(),
                "trajectory batch exceeds 1 MiB CPU ingestion bound"
            );
            // The wide update's size check before any read takes the
            // configured bound too.
            let unsealed = unsealed_over_1_mib_with_bogus_hash(&root, "w-unsealed.json");
            assert_eq!(
                execute_v1(prepared(&wide, &unsealed, 1, root.join("meta-small"))).unwrap_err(),
                "trajectory batch exceeds 1 MiB CPU ingestion bound"
            );
        })
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

/// Every prepared update hands the physical-group bound of its own batch
/// bound (`preparation_groups_bound_v1`) to its preparation: net-8's
/// `UpdatePrepared` and prepared `Ex1Update`, the S learner's reference
/// preparation and the wide learner's. A batch of 65,537 physical groups, one
/// more than the historical bound, is refused as such at the default batch
/// bound. At 513 MiB (65,664 groups) it is planned and dispatched, and its
/// replay stops at group 0: no row of this batch replays. With the historical
/// 65,536 in place of the configured bound at the command, at a learner's
/// call or inside the preparation, the 513 MiB update is refused at planning
/// or at dispatch instead.
#[test]
fn prepared_updates_forward_the_scaled_physical_group_bound() {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            assert_eq!(
                preparation_groups_bound_v1(MAX_BATCH_BYTES),
                HISTORICAL_MAX_GROUPS
            );
            assert_eq!(
                preparation_groups_bound_v1(trajectory_batch_bytes_v1(513)),
                HISTORICAL_MAX_GROUPS + 128
            );
            let root = TempTestDir::new_v1("prepared-physical-group-bound");
            let net8 = s_update_fixture_v1::net8_source_v1(&root).unwrap();
            let s = s_update_fixture_v1::s_source_v1(
                &root,
                &net8,
                s_update_fixture_v1::TEST_CONFIG_V1,
                s_update_fixture_v1::S_INIT_SEED_V1,
            )
            .unwrap();
            let wide = fixed_wide_source(&root, &net8);
            // A pairing relation, which only a FlatV8 policy replays: every
            // row is refused before any forward of it.
            let row = DecisionRecordV1 {
                step: 0,
                physical_decision_id: 0,
                substep_index: 0,
                substep_count: 1,
                actor: 0,
                selected: 0,
                logits: vec![0.0_f32.to_bits()],
                value: 0.0_f32.to_bits(),
                tensor: TensorBitsV1 {
                    pairing_node_indices: vec![0, 1],
                    ..empty_tensor_bits()
                },
                sampler_identity: decision_sampler_identity_v1(1).map(str::to_owned),
            };
            let counts = [32_769, 32_768];
            assert_eq!(counts.iter().sum::<usize>(), HISTORICAL_MAX_GROUPS + 1);
            let batch = |label: &str, source: &ExpandedModelSourceV1| {
                sealed_synthetic_group_batch(
                    &root.join(format!("groups-{label}")),
                    source,
                    &counts,
                    &row,
                )
            };
            let check =
                |label: &str, command: &dyn Fn(usize, PathBuf) -> ExpandedTrainingCommandV1| {
                    assert_eq!(
                        execute_v1(command(
                            DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
                            root.join(format!("{label}-default")),
                        ))
                        .unwrap_err(),
                        "preparation exceeds 65536 physical groups",
                        "{label} at the default batch bound"
                    );
                    assert_eq!(
                        execute_v1(command(513, root.join(format!("{label}-raised")))).unwrap_err(),
                        "preparation group 0: only a FlatV8 policy replays a pairing relation",
                        "{label} at 513 MiB"
                    );
                };
            let net8_pins = batch("net8", &net8);
            check("net8", &|mebibytes, output_directory| {
                prepared(&net8, &net8_pins, mebibytes, output_directory)
            });
            // The prepared `Ex1Update` stops in its preparation too, before
            // it reads its collection.
            check("net8-ex1", &|mebibytes, output_directory| {
                ExpandedTrainingCommandV1::Ex1Update {
                    source: net8.clone(),
                    trajectories: net8_pins.clone(),
                    learning_rate: 0.0001,
                    value_coefficient: 0.5,
                    loss_selection: gae(),
                    ex1: ex1_update_v1::Ex1UpdateOptionsV1 {
                        run: "batch-bound-groups".into(),
                        update: 0,
                        coefficient: 1.0,
                        cap: search_targets_v1::SearchTargetCapV1::Ratio { kappa: 2.0 },
                        collection: PinnedFileV1 {
                            path: root.join("no-collection.json"),
                            sha256: "1".repeat(64),
                        },
                        w5: None,
                    },
                    preparation_workers: Some(2),
                    max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
                    max_trajectory_batch_mebibytes: mebibytes,
                    record_envelope: RecordEnvelopeV1::None,
                    output_directory,
                }
            });
            let s_pins = batch("s", &s);
            check("s", &|mebibytes, output_directory| {
                prepared(&s, &s_pins, mebibytes, output_directory)
            });
            let wide_pins = batch("wide", &wide);
            check("wide", &|mebibytes, output_directory| {
                prepared(&wide, &wide_pins, mebibytes, output_directory)
            });
        })
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

/// The verified prepared replay command (`UpdateSVerifiedPrepared`) hands the
/// same bound to its verified learner's own preparation (`s_learner_v1` to
/// `verified_replay_v1::prepared_gradient_v1` to
/// `prepare_deferring_learner_rows_v1`). A failure there falls back to the
/// reference, so this batch is one the verified learner completes: 65,537
/// physical groups of one learner row each, a decision S scores, with S's own
/// outputs. It is refused at the default batch bound; at 513 MiB the update
/// completes through the verified learner with every group. With the
/// historical 65,536 at either call, the verified learner's preparation is
/// refused and the reference runs in its place (`used` false).
#[test]
fn s_verified_prepared_update_forwards_the_scaled_physical_group_bound() {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let root = TempTestDir::new_v1("s-verified-physical-group-bound");
            let net8 = s_update_fixture_v1::net8_source_v1(&root).unwrap();
            let s = s_update_fixture_v1::s_source_v1(
                &root,
                &net8,
                s_update_fixture_v1::TEST_CONFIG_V1,
                s_update_fixture_v1::S_INIT_SEED_V1,
            )
            .unwrap();
            // The smallest decision S scores: one object and one action.
            let tensor = TensorBitsV1 {
                state: vec![0; crate::s_transformer_v1::STATE_DIM_V1],
                object_features: vec![0; crate::s_transformer_v1::OBJECT_FEATURE_DIM_V1],
                object_card_ids: vec![0],
                object_groups: vec![0],
                object_node_ids: vec![0],
                action_features: vec![0; crate::s_transformer_v1::ACTION_FEATURE_DIM_V1],
                ..empty_tensor_bits()
            };
            let (policy, _) = load_expanded_inference_any_math_v1(&s).unwrap();
            let (logits, value) = policy
                .s_model_v1()
                .unwrap()
                .score_tensor_v1(&tensor.tensor())
                .unwrap();
            assert_eq!(logits.len(), 1);
            let row = DecisionRecordV1 {
                step: 0,
                physical_decision_id: 0,
                substep_index: 0,
                substep_count: 1,
                actor: 0,
                selected: 0,
                logits: bits(&logits),
                value: value.to_bits(),
                tensor,
                sampler_identity: decision_sampler_identity_v1(1).map(str::to_owned),
            };
            let counts = [32_769, 32_768];
            assert_eq!(counts.iter().sum::<usize>(), HISTORICAL_MAX_GROUPS + 1);
            let pins = sealed_synthetic_group_batch(&root.join("groups"), &s, &counts, &row);
            let verified = |mebibytes: usize, name: &str| {
                execute_v1(ExpandedTrainingCommandV1::UpdateSVerifiedPrepared {
                    source: s.clone(),
                    trajectories: pins.clone(),
                    learning_rate: 0.0001,
                    value_coefficient: 0.5,
                    loss_selection: gae(),
                    preparation_workers: 2,
                    max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
                    max_trajectory_batch_mebibytes: mebibytes,
                    tape_cap_bytes: 16 << 20,
                    output_directory: root.join(name),
                })
            };
            assert_eq!(
                verified(DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES, "default").unwrap_err(),
                "preparation exceeds 65536 physical groups"
            );
            let updated = verified(513, "raised").unwrap();
            assert_eq!(updated["complete"], true);
            assert_eq!(updated["verified_replay"]["used"], true, "{updated}");
            assert_eq!(
                updated["update_preparation"]["physical_group_jobs"],
                HISTORICAL_MAX_GROUPS + 1
            );
            assert_eq!(
                updated["update_preparation"]["deferred_learner_rows"],
                HISTORICAL_MAX_GROUPS + 1
            );
        })
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

/// Absent and the default share wire bytes; a raised bound is serialized and
/// read back as itself; an absent key reads back as 512.
#[test]
fn the_prepared_command_keeps_its_wire_bytes_at_the_default_bound() {
    let source = ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path: "import.json".into(),
            sha256: "0".repeat(64),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: "c".into(),
            expected_feature_encoding_digest: "e".into(),
        },
        checkpoint: None,
        s_learner: None,
        w_learner: None,
    };
    let default = serde_json::to_string(&prepared(
        &source,
        &[],
        DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
        "out".into(),
    ))
    .unwrap();
    assert!(!default.contains("max_trajectory_batch_mebibytes"));
    let back: ExpandedTrainingCommandV1 = serde_json::from_str(&default).unwrap();
    match &back {
        ExpandedTrainingCommandV1::UpdatePrepared {
            max_trajectory_batch_mebibytes,
            ..
        } => assert_eq!(*max_trajectory_batch_mebibytes, 512),
        _ => unreachable!(),
    }
    assert_eq!(serde_json::to_string(&back).unwrap(), default);
    let raised = serde_json::to_string(&prepared(&source, &[], 2048, "out".into())).unwrap();
    let member = "\"max_trajectory_batch_mebibytes\":2048,";
    assert!(raised.contains(member));
    assert_eq!(raised.replace(member, ""), default);
    let back: ExpandedTrainingCommandV1 = serde_json::from_str(&raised).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), raised);

    // `Ex1Update` and `UpdateSVerifiedPrepared` follow the same default wire
    // rule and validation gates.
    let ex1_command = |workers, mebibytes| ExpandedTrainingCommandV1::Ex1Update {
        source: source.clone(),
        trajectories: Vec::new(),
        learning_rate: 0.0001,
        value_coefficient: 0.5,
        loss_selection: gae(),
        ex1: ex1_update_v1::Ex1UpdateOptionsV1 {
            run: "batch-bound-wire".into(),
            update: 0,
            coefficient: 1.0,
            cap: search_targets_v1::SearchTargetCapV1::Ratio { kappa: 2.0 },
            collection: PinnedFileV1 {
                path: "collection.json".into(),
                sha256: "1".repeat(64),
            },
            w5: None,
        },
        preparation_workers: workers,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        max_trajectory_batch_mebibytes: mebibytes,
        record_envelope: RecordEnvelopeV1::None,
        output_directory: "out".into(),
    };
    let ex1_default = serde_json::to_string(&ex1_command(
        Some(2),
        DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES,
    ))
    .unwrap();
    assert!(!ex1_default.contains("max_trajectory_batch_mebibytes"));
    let ex1_back: ExpandedTrainingCommandV1 = serde_json::from_str(&ex1_default).unwrap();
    match &ex1_back {
        ExpandedTrainingCommandV1::Ex1Update {
            max_trajectory_batch_mebibytes,
            ..
        } => assert_eq!(*max_trajectory_batch_mebibytes, 512),
        _ => unreachable!(),
    }
    let ex1_raised = serde_json::to_string(&ex1_command(Some(2), 2048)).unwrap();
    assert!(ex1_raised.contains(member));
    assert_eq!(ex1_raised.replace(member, ""), ex1_default);
    assert_eq!(
        execute_v1(ex1_command(None, 2048)).unwrap_err(),
        "max_trajectory_batch_mebibytes applies to prepared updates only"
    );
    for refused in [0, 4097] {
        assert_eq!(
            execute_v1(ex1_command(Some(2), refused)).unwrap_err(),
            "max_trajectory_batch_mebibytes must be in 1..=4096"
        );
    }

    let verified_command =
        |workers, mebibytes| ExpandedTrainingCommandV1::UpdateSVerifiedPrepared {
            source: source.clone(),
            trajectories: Vec::new(),
            learning_rate: 0.0001,
            value_coefficient: 0.5,
            loss_selection: gae(),
            preparation_workers: workers,
            max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            max_trajectory_batch_mebibytes: mebibytes,
            tape_cap_bytes: 1 << 20,
            output_directory: "out".into(),
        };
    let verified_default =
        serde_json::to_string(&verified_command(2, DEFAULT_MAX_TRAJECTORY_BATCH_MEBIBYTES))
            .unwrap();
    assert!(!verified_default.contains("max_trajectory_batch_mebibytes"));
    let verified_back: ExpandedTrainingCommandV1 = serde_json::from_str(&verified_default).unwrap();
    match &verified_back {
        ExpandedTrainingCommandV1::UpdateSVerifiedPrepared {
            max_trajectory_batch_mebibytes,
            ..
        } => assert_eq!(*max_trajectory_batch_mebibytes, 512),
        _ => unreachable!(),
    }
    let verified_raised = serde_json::to_string(&verified_command(2, 2048)).unwrap();
    assert!(verified_raised.contains(member));
    assert_eq!(verified_raised.replace(member, ""), verified_default);
    assert_eq!(
        execute_v1(verified_command(1, 2048)).unwrap_err(),
        "max_trajectory_batch_mebibytes applies to prepared updates only (preparation_workers > 1)"
    );
    for refused in [0, 4097] {
        assert_eq!(
            execute_v1(verified_command(2, refused)).unwrap_err(),
            "max_trajectory_batch_mebibytes must be in 1..=4096"
        );
    }
}
