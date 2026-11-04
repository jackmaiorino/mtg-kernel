//! The S learner in RT1: wire bytes without it, collection against net-8
//! and S opponents, the GAE update, checkpoint readback, the run driver
//! (resumed, revalidated, serial equal to parallel), and the refusals.

use super::*;
use crate::expanded_deck_training_v1::{
    execute_v1, initialize_any_math_v1, read_pinned, sha,
    write_synthetic_fresh_source_with_parameters_v1, ExpandedDeckListV1, ExpandedEpisodeV1,
    ExpandedTrainingCommandV1, DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
};
use crate::native_expanded_training_run_v1::{
    run_native_expanded_training_v1, ExpandedOpponentAssignmentV1, ExpandedRunIterationV1,
    NamedExpandedOpponentV1, NativeExpandedTrainingRunV1, ScheduledExpandedEpisodeV1,
};
use crate::s_transformer_v1::{SConfigV1, SModelV1};
use crate::sideboard_play_policy_v1::FrozenPlayObservationTransferV3;
use std::path::Path;

/// A small S for tests; the card table keeps S's 257 rows.
const SMALL_V1: SConfigV1 = SConfigV1 {
    d_model: 8,
    layers: 1,
    heads: 2,
    ffn: 16,
    card_dim: 4,
    card_vocab: 257,
};

fn on_large_stack(body: fn()) {
    let run = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(body)
        .unwrap();
    if let Err(payload) = run.join() {
        std::panic::resume_unwind(payload);
    }
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "s-learner-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

/// A fresh-initialization net-8 source (V4) without a checkpoint: the
/// net-8 opponent, and the S learner's shell.
fn net8_source(root: &Path) -> ExpandedModelSourceV1 {
    let identity = FRESH_FEATURE_IDENTITY_V4;
    let descriptor =
        write_synthetic_fresh_source_with_parameters_v1(&root.join("net8"), identity, |_| {});
    let bytes = serde_json::to_vec(&descriptor).unwrap();
    let path = root.join("net8-descriptor.json");
    fs::write(&path, &bytes).unwrap();
    ExpandedModelSourceV1 {
        play_import: PinnedFileV1 {
            path,
            sha256: sha(&bytes),
        },
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
    }
}

/// `shell` with an S learner whose initial state carries nonzero Adam
/// moments and step, which the fresh start must drop.
fn s_source(root: &Path, shell: &ExpandedModelSourceV1) -> ExpandedModelSourceV1 {
    let model = SModelV1::init_pilot_scheme_v1(SMALL_V1, 20_261_103).unwrap();
    let mut grad = vec![1.0e-3f32; model.values_v1().len()];
    for range in model.layout_v1().gauge_ranges_v1() {
        grad[range].fill(0.0);
    }
    let mut state = STrainStateV1::fresh_optimizer_v1(model);
    // One step with a nonzero gradient moves the parameters and leaves
    // nonzero moments and step 1, so the start's reset is visible.
    state.adam_step_with_gradient_v1(&grad, 1.0e-4).unwrap();
    assert_eq!(state.adam_step_v1(), 1);
    assert!(state.first_moments_v1().iter().any(|m| *m != 0.0));
    assert!(state.second_moments_v1().iter().any(|m| *m != 0.0));
    let bytes = encode_checkpoint_v1(&state);
    let path = root.join("s0.bin");
    fs::write(&path, &bytes).unwrap();
    ExpandedModelSourceV1 {
        s_learner: Some(Box::new(SLearnerSourceV1 {
            schema: S_LEARNER_SOURCE_SCHEMA_V1.into(),
            initial: PinnedFileV1 {
                path,
                sha256: sha(&bytes),
            },
        })),
        ..shell.clone()
    }
}

fn list(id: &str) -> ExpandedDeckListV1 {
    let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let cards = registration.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    }
}

fn episode(id: &str, seed: u64, opponent: Option<ExpandedModelSourceV1>) -> ExpandedEpisodeV1 {
    let decks = [list("Affinity"), list("Terror")];
    ExpandedEpisodeV1 {
        id: id.into(),
        seed,
        starting_player: 0,
        learner_seat: 0,
        opponent,
        opponent_kind: None,
        opponent_search: None,
        registered: decks.clone(),
        selected: decks,
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
        opening: None,
        mulligan_modes: None,
    }
}

fn gae() -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
    }
}

fn collect(
    source: &ExpandedModelSourceV1,
    episodes: Vec<ExpandedEpisodeV1>,
    output_directory: PathBuf,
) -> Result<Value, String> {
    execute_v1(ExpandedTrainingCommandV1::Collect {
        source: source.clone(),
        episodes,
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        ex1: None,
        output_directory,
    })
}

fn update(
    source: &ExpandedModelSourceV1,
    trajectories: Vec<PinnedFileV1>,
    loss_selection: ExpandedLossSelectionV1,
    record_format: CheckpointRecordFormatV1,
    output_directory: PathBuf,
) -> Result<Value, String> {
    execute_v1(ExpandedTrainingCommandV1::Update {
        source: source.clone(),
        trajectories,
        learning_rate: 1.0e-3,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection,
        line_b: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format,
        output_directory,
    })
}

#[test]
fn a_source_without_s_learner_keeps_its_wire_bytes() {
    let legacy = concat!(
        r#"{"play_import":{"path":"/models/import.json","sha256":"#,
        r#""0000000000000000000000000000000000000000000000000000000000000001"},"#,
        r#""feature_transfer":{"expected_feature_contract_digest":"c","#,
        r#""expected_feature_encoding_digest":"e"},"checkpoint":null}"#
    );
    let source: ExpandedModelSourceV1 = serde_json::from_str(legacy).unwrap();
    assert!(source.s_learner.is_none());
    assert_eq!(serde_json::to_string(&source).unwrap(), legacy);
    let with = ExpandedModelSourceV1 {
        s_learner: Some(Box::new(SLearnerSourceV1 {
            schema: S_LEARNER_SOURCE_SCHEMA_V1.into(),
            initial: source.play_import.clone(),
        })),
        ..source.clone()
    };
    let text = serde_json::to_string(&with).unwrap();
    assert!(text.ends_with(
        r#""checkpoint":null,"s_learner":{"schema":"w5-s-learner/v1","initial":{"path":"/models/import.json","sha256":"0000000000000000000000000000000000000000000000000000000000000001"}}}"#
    ));
    assert_eq!(
        serde_json::from_str::<ExpandedModelSourceV1>(&text).unwrap(),
        with
    );
    // An explicit null is absence, as for every optional member.
    let null = legacy.replace(
        r#""checkpoint":null}"#,
        r#""checkpoint":null,"s_learner":null}"#,
    );
    let parsed: ExpandedModelSourceV1 = serde_json::from_str(&null).unwrap();
    assert_eq!(serde_json::to_string(&parsed).unwrap(), legacy);
}

/// End to end on real games: S collects against itself and a net-8
/// opponent, updates once (fresh Adam from the initial parameters),
/// publishes a checkpoint that resumes; the run driver then trains two
/// iterations against the current S and the fixed net-8 opponent, resumes
/// after the first and revalidates, and its parallel collection and prepared
/// update reach the serial run's state.
#[test]
fn s_learner_collects_updates_and_runs_against_net8_opponents() {
    on_large_stack(s_learner_end_to_end_v1);
}

fn s_learner_end_to_end_v1() {
    let root = temp_root("e2e");
    let net8 = net8_source(&root);
    let s = s_source(&root, &net8);

    // The fresh start: the initial parameters with step 0 and zero moments.
    let (policy, identity) = load_inference_v1(&s).unwrap();
    assert_eq!(identity.adam_step, 0);
    let initial =
        decode_checkpoint_v1(&fs::read(&s.s_learner.as_ref().unwrap().initial.path).unwrap())
            .unwrap();
    let fresh = STrainStateV1::fresh_optimizer_v1(initial.model_v1().clone());
    assert_eq!(identity.state_sha256, fresh.state_sha256_v1());
    assert_eq!(
        identity.model.weights_sha256,
        initial.model_v1().parameter_sha256_v1()
    );
    assert!(policy.feature_identity_v1() == FRESH_FEATURE_IDENTITY_V4);
    drop(policy);
    let loaded = load_s_v1(&s, None).unwrap();
    assert_eq!(loaded.state.adam_step_v1(), 0);
    assert_eq!(
        loaded.state.model_v1().values_v1(),
        initial.model_v1().values_v1()
    );
    assert!(loaded
        .state
        .first_moments_v1()
        .iter()
        .all(|m| m.to_bits() == 0));
    assert!(loaded
        .state
        .second_moments_v1()
        .iter()
        .all(|m| m.to_bits() == 0));
    assert!(initial.first_moments_v1().iter().any(|m| *m != 0.0));
    drop(loaded);

    let collection = collect(
        &s,
        vec![
            episode("s-self", 2_026_110_301, None),
            episode("s-vs-net8", 2_026_110_302, Some(net8.clone())),
        ],
        root.join("collect"),
    )
    .unwrap();
    assert_eq!(collection["behavior_state_sha256"], identity.state_sha256);
    let trajectories: Vec<PinnedFileV1> =
        serde_json::from_value(collection["trajectories"].clone()).unwrap();
    let crossed: super::super::ExpandedTrajectoryV1 = read_pinned(&trajectories[1]).unwrap();
    let behaviors = crossed.seat_behaviors.as_ref().unwrap();
    assert_eq!(behaviors[0].identity, identity);
    assert_eq!(behaviors[1].source, net8);
    assert_ne!(
        behaviors[1].identity.model.weights_sha256,
        identity.model.weights_sha256
    );

    let updated = update(
        &s,
        trajectories.clone(),
        gae(),
        CheckpointRecordFormatV1::V1,
        root.join("update"),
    )
    .unwrap();
    assert_eq!(updated["adam_step"], 1);
    assert_eq!(updated["before_state_sha256"], identity.state_sha256);
    assert_ne!(updated["after_state_sha256"], identity.state_sha256);
    assert_eq!(updated["numerical_backend"], "native-cpu-sequential");
    assert!(updated["loss"].as_f64().unwrap().is_finite());
    let checkpoint: PinnedFileV1 = serde_json::from_value(updated["checkpoint"].clone()).unwrap();
    let next = ExpandedModelSourceV1 {
        checkpoint: Some(checkpoint.clone()),
        ..s.clone()
    };
    let (_, after) = load_inference_v1(&next).unwrap();
    assert_eq!(after.adam_step, 1);
    assert_eq!(updated["after_state_sha256"], after.state_sha256);
    assert_ne!(after.model.weights_sha256, identity.model.weights_sha256);
    let record: SLearnerCheckpointV1 = read_pinned(&checkpoint).unwrap();
    assert_eq!(record.trajectories, trajectories);
    // An ordinary record has no `ex1` member (PR A's fields exactly).
    assert!(record.ex1.is_none());
    let bytes = fs::read(&checkpoint.path).unwrap();
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(document.get("ex1").is_none());
    assert_eq!(serde_json::to_value(&record).unwrap(), document);
    // PR A's (239cfad50) record members under the platform's libm, literally.
    let mut keys: Vec<&str> = document
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut pr_a = vec![
        "schema",
        "architecture",
        "initial_sha256",
        "source_import",
        "feature_contract_digest",
        "feature_encoding_digest",
        "card_db_hash",
        "state",
        "state_sha256",
        "adam_step",
        "trajectories",
        "loss_identity",
        "gamma_bits",
        "gae_lambda_bits",
        "entropy_coefficient_bits",
        "learning_rate_bits",
        "value_coefficient_bits",
    ];
    pr_a.sort_unstable();
    keys.sort_unstable();
    assert_eq!(keys, pr_a);
    let restored = decode_checkpoint_v1(&fs::read(&record.state.path).unwrap()).unwrap();
    assert_eq!(restored.state_sha256_v1(), after.state_sha256);
    // The run driver's continuation read binds the update's inputs.
    let expectation = CheckpointInputExpectationV1 {
        trajectories: &trajectories,
        learning_rate_bits: 1.0e-3f32.to_bits(),
        value_coefficient_bits: 0.5f32.to_bits(),
    };
    assert_eq!(
        load_update_inference_v1(&next, &expectation).unwrap(),
        after
    );
    let wrong = CheckpointInputExpectationV1 {
        learning_rate_bits: 2.0e-3f32.to_bits(),
        ..expectation
    };
    assert!(load_update_inference_v1(&next, &wrong).is_err());
    // The recovery check binds the run's GAE settings exactly.
    validate_update_loss_v1(&next, &gae()).unwrap();
    for loss in [
        ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 0.99,
            lambda: 0.9,
            entropy_coefficient: 0.0,
        },
        ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 1.0,
            lambda: 0.95,
            entropy_coefficient: 0.0,
        },
        ExpandedLossSelectionV1::TerminalReinforceValueV3,
    ] {
        assert!(validate_update_loss_v1(&next, &loss).is_err());
    }
    validate_update_loss_v1(&net8, &ExpandedLossSelectionV1::TerminalReinforceValueV3).unwrap();
    // A continuation with other GAE settings is refused.
    assert!(update(
        &next,
        Vec::from([PinnedFileV1 {
            path: root.join("missing.json"),
            sha256: "0".repeat(64),
        }]),
        ExpandedLossSelectionV1::GaeAdvantageValueV1 {
            gamma: 0.99,
            lambda: 0.9,
            entropy_coefficient: 0.0,
        },
        CheckpointRecordFormatV1::V1,
        root.join("other-gamma"),
    )
    .unwrap_err()
    .contains("gamma and lambda"));
    // The same update again reproduces the same state.
    let again = update(
        &s,
        trajectories.clone(),
        gae(),
        CheckpointRecordFormatV1::V1,
        root.join("update-again"),
    )
    .unwrap();
    assert_eq!(again["after_state_sha256"], updated["after_state_sha256"]);
    // A stale batch is refused at the next state.
    assert!(update(
        &next,
        trajectories,
        gae(),
        CheckpointRecordFormatV1::V1,
        root.join("stale"),
    )
    .unwrap_err()
    .contains("stale trajectory"));

    // The run driver: parallel collection and prepared updates, resumed and
    // revalidated, then the serial run.
    let parallel = run_config(&s, &net8, 2, root.join("run-parallel"));
    let first = run_native_expanded_training_v1(&parallel, Some(1)).unwrap();
    assert_eq!(first["completed_iterations"], 1);
    let finished = run_native_expanded_training_v1(&parallel, None).unwrap();
    assert_eq!(finished["complete"], true);
    assert_eq!(finished["actual_identity"]["adam_step"], 2);
    let revalidated = run_native_expanded_training_v1(&parallel, None).unwrap();
    assert_eq!(revalidated, finished);
    let serial = run_config(&s, &net8, 1, root.join("run-serial"));
    let serial_result = run_native_expanded_training_v1(&serial, None).unwrap();
    for field in ["state_sha256", "model", "adam_step"] {
        assert_eq!(
            serial_result["actual_identity"][field],
            finished["actual_identity"][field]
        );
    }
    let _ = fs::remove_dir_all(&root);
}

fn run_config(
    initial_source: &ExpandedModelSourceV1,
    net8: &ExpandedModelSourceV1,
    workers: usize,
    output_directory: PathBuf,
) -> NativeExpandedTrainingRunV1 {
    let scheduled = |name: &str, seed: u64, opponent| ScheduledExpandedEpisodeV1 {
        episode: episode(name, seed, None),
        opponent,
    };
    let fixed = || ExpandedOpponentAssignmentV1::Fixed { id: "net8".into() };
    NativeExpandedTrainingRunV1 {
        schema: "mtg-kernel-native-expanded-training-run/v1".into(),
        initial_source: initial_source.clone(),
        opponents: vec![NamedExpandedOpponentV1 {
            id: "net8".into(),
            source: net8.clone(),
        }],
        iterations: vec![
            ExpandedRunIterationV1 {
                episodes: vec![
                    scheduled(
                        "iter-0-self",
                        2_026_110_311,
                        ExpandedOpponentAssignmentV1::Current,
                    ),
                    scheduled("iter-0-net8", 2_026_110_312, fixed()),
                ],
            },
            ExpandedRunIterationV1 {
                episodes: vec![
                    scheduled(
                        "iter-1-self",
                        2_026_110_313,
                        ExpandedOpponentAssignmentV1::Current,
                    ),
                    scheduled(
                        "iter-1-initial",
                        2_026_110_314,
                        ExpandedOpponentAssignmentV1::Initial,
                    ),
                ],
            },
        ],
        record_format: Default::default(),
        learning_rate: 1.0e-3,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        loss_selection: gae(),
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        collection_workers: workers,
        preparation_workers: workers,
        max_non_natural_episode_fraction: 0.0,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        line_b: None,
        e_supported: None,
        soft_q: None,
        search_targets: None,
        ex1: None,
        record_process_cpu: false,
        chain_ledger: None,
        output_directory,
    }
}

#[test]
fn s_learner_refuses_unsupported_options() {
    on_large_stack(s_learner_refusals_v1);
}

fn s_learner_refusals_v1() {
    let root = temp_root("refusals");
    let net8 = net8_source(&root);
    let s = s_source(&root, &net8);
    // Net-8-only loaders refuse an S source.
    match initialize_any_math_v1(&s) {
        Ok(_) => panic!("the net-8 loader admitted an S source"),
        Err(error) => assert_eq!(error, S_LEARNER_NET8_REFUSAL_V1),
    }
    // FlatV7 and FlatV8 shells.
    let v7 = ExpandedModelSourceV1 {
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest:
                crate::native_flat_tensorizer_v7::FEATURE_CONTRACT_DIGEST_V7.into(),
            expected_feature_encoding_digest:
                crate::native_flat_tensorizer_v7::FEATURE_ENCODING_DIGEST_V7.into(),
        },
        ..s.clone()
    };
    assert!(load_inference_v1(&v7).err().unwrap().contains("FlatV7"));
    // An unknown member schema.
    let mut unknown = s.clone();
    unknown.s_learner.as_mut().unwrap().schema = "w5-s-learner/v0".into();
    assert!(load_inference_v1(&unknown).is_err());
    // Net-8 entry points that do not go through the initializer.
    assert_eq!(
        super::super::inspect_fresh_initialization_v1(&s, &root.join("inspect")).unwrap_err(),
        S_LEARNER_NET8_REFUSAL_V1
    );
    let with_checkpoint = ExpandedModelSourceV1 {
        checkpoint: Some(s.s_learner.as_ref().unwrap().initial.clone()),
        ..s.clone()
    };
    assert_eq!(
        super::super::validate_ordinary_source_descriptor_v1(&with_checkpoint).unwrap_err(),
        S_LEARNER_NET8_REFUSAL_V1
    );
    // Collection options.
    assert!(collection_learner_v1(
        &s,
        TrainingMathV1::PlatformLibm,
        CollectionSamplerV1::UnclampedSoftmaxF64IcdfU53V1,
    )
    .err()
    .unwrap()
    .contains("legacy sampler"));
    // Update options, refused before any input is read.
    let fake = vec![PinnedFileV1 {
        path: root.join("missing.json"),
        sha256: "0".repeat(64),
    }];
    let refused = |loss, format, name: &str| {
        update(&s, fake.clone(), loss, format, root.join(name)).unwrap_err()
    };
    assert!(refused(
        ExpandedLossSelectionV1::TerminalReinforceValueV3,
        CheckpointRecordFormatV1::V1,
        "terminal"
    )
    .contains("GAE loss"));
    assert!(refused(gae(), CheckpointRecordFormatV1::V2, "v2").contains("record format v2"));
    let error = execute_v1(ExpandedTrainingCommandV1::Update {
        source: s.clone(),
        trajectories: fake.clone(),
        learning_rate: 1.0e-3,
        value_coefficient: 0.5,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: gae(),
        line_b: Some(super::super::LineBUpdateOptionsV1 {
            optimizer_mask: super::super::OptimizerMaskV1::LineBHeadOnlyV1,
            teacher: None,
        }),
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::V1,
        output_directory: root.join("line-b"),
    })
    .unwrap_err();
    assert!(error.contains("line (b)"), "{error}");
    // The run driver refuses the same before any work.
    let mut config = run_config(&s, &net8, 1, root.join("run"));
    config.loss_selection = ExpandedLossSelectionV1::TerminalReinforceValueV3;
    assert!(config.validate_v1().unwrap_err().contains("S learner run"));
    let _ = fs::remove_dir_all(&root);
}

/// A chain id whose W5 placement draws the 1-based eligible index at most
/// 16 for every episode of [`run_config`]'s two iterations, so each episode
/// searches early in its game (the draw is a pure hash; any id works, this
/// one only makes the searched roots likely).
fn early_w5_chain_v1() -> String {
    let seeds = [
        (0, 2_026_110_311, 0),
        (0, 2_026_110_312, 1),
        (1, 2_026_110_313, 0),
        (1, 2_026_110_314, 1),
    ];
    (0..1_000_000)
        .map(|k| format!("w5/s-chain-{k}"))
        .find(|chain| {
            seeds.iter().all(|&(update, seed, slot)| {
                crate::w5_xact_v1::w5_placement_index_v1(crate::w5_xact_v1::w5_placement_hash_v1(
                    chain, update, seed, slot,
                )) <= 16
            })
        })
        .expect("a chain id with early placements")
}

fn w5_options_v1(chain: &str) -> crate::native_expanded_training_run_v1::Ex1RunOptionsV1 {
    crate::native_expanded_training_run_v1::Ex1RunOptionsV1 {
        run: chain.into(),
        update_offset: 0,
        coefficient: 1.0,
        cap: super::super::search_targets_v1::SearchTargetCapV1::Ratio { kappa: 2.0 },
        w5: Some(crate::native_expanded_training_run_v1::W5RunOptionsV1 {
            budget: 8_000,
            key_memo_mib: 512,
        }),
    }
}

/// W5 (`w5-xact-v1`) with S as the learner through the production run
/// path: the hook searches with S, the update takes the forward-KL rows,
/// masks the search-played substeps' policy gradient and runs step
/// telemetry on update 0 only; the checkpoint record carries the W5
/// continuation and next global update; the run resumes, revalidates and
/// replays to the same states; an ordinary continuation of the W5 lineage is
/// refused.
#[test]
fn s_learner_runs_the_w5_recipe() {
    on_large_stack(s_learner_w5_v1);
}

fn s_learner_w5_v1() {
    let root = temp_root("w5");
    let net8 = net8_source(&root);
    let s = s_source(&root, &net8);
    let chain = early_w5_chain_v1();
    let config = |label: &str| {
        let mut config = run_config(&s, &net8, 1, root.join(label));
        config.ex1 = Some(w5_options_v1(&chain));
        config.validate_v1().unwrap();
        config
    };
    let first = config("run-a");
    let partial = run_native_expanded_training_v1(&first, Some(1)).unwrap();
    assert_eq!(partial["completed_iterations"], 1);
    let result = run_native_expanded_training_v1(&first, None).unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(result["actual_identity"]["adam_step"], 2);
    assert_eq!(
        run_native_expanded_training_v1(&first, None).unwrap(),
        result
    );
    let mut states = Vec::new();
    let mut masked = 0;
    let mut placements = Vec::new();
    let mut last_checkpoint = None;
    for i in 0..2 {
        let pin: PinnedFileV1 = serde_json::from_value(result["iterations"][i].clone()).unwrap();
        let receipt: Value = read_pinned(&pin).unwrap();
        let update_pin: PinnedFileV1 = serde_json::from_value(receipt["update"].clone()).unwrap();
        let update: Value = read_pinned(&update_pin).unwrap();
        assert_eq!(update["s_learner"]["architecture"], S_ARCHITECTURE_V1);
        let ex1_pin: PinnedFileV1 =
            serde_json::from_value(update["ex1"]["receipt"].clone()).unwrap();
        let ex1: Value = read_pinned(&ex1_pin).unwrap();
        assert_eq!(ex1["method"], "w5-xact-v1");
        assert_eq!(ex1["w5"]["budget"], 8_000);
        assert_eq!(ex1["w5"]["step_telemetry"], json!(i == 0), "update {i}");
        let exposure = &ex1["gradient_exposure"];
        assert_eq!(exposure["learner"], S_ARCHITECTURE_V1);
        // Every field net-8's W5 exposure records.
        for key in [
            "rows",
            "denominator",
            "coefficient",
            "kl_before",
            "auxiliary_loss",
            "total_loss",
            "gradient_components",
            "auxiliary_head_l2",
            "ordinary_head_l2",
            "auxiliary_bias_residual",
            "cap",
            "adam_steps",
            "movement",
            "thresholds",
            "pg_masked_rows",
            "step_telemetry",
        ] {
            assert!(exposure.get(key).is_some(), "S exposure lacks {key}");
        }
        for key in [
            "method",
            "receipt",
            "collection",
            "rows",
            "cap",
            "adam_steps",
        ] {
            assert!(update["ex1"].get(key).is_some(), "S update lacks ex1.{key}");
        }
        let played = ex1["w5"]["pg_masked"].as_array().unwrap().len();
        assert_eq!(exposure["pg_masked_rows"], json!(played));
        assert_eq!(exposure["rows"], ex1["rows"]);
        let rows = ex1["rows"].as_u64().unwrap();
        // Telemetry: update 0, and any update with no rows.
        assert_eq!(exposure["step_telemetry"], json!(i == 0 || rows == 0));
        assert_eq!(exposure["adam_steps"].is_null(), !(i == 0 || rows == 0));
        assert_eq!(exposure["cap"]["mode"], "ratio");
        masked += played;
        placements.push(ex1["w5"]["placements"].clone());
        let checkpoint: PinnedFileV1 =
            serde_json::from_value(update["checkpoint"].clone()).unwrap();
        let record: SLearnerCheckpointV1 = read_pinned(&checkpoint).unwrap();
        let saved = record.ex1.as_ref().unwrap();
        assert_eq!(saved["w5_next_update"], json!(i + 1));
        assert_eq!(saved["receipt"], json!(ex1_pin));
        states.push(update["after_state_sha256"].clone());
        last_checkpoint = Some(checkpoint);
    }
    assert!(
        masked > 0,
        "no update masked a search-played row: {placements:?}"
    );
    // The same run again reaches the same states.
    let again = run_native_expanded_training_v1(&config("run-b"), None).unwrap();
    for field in ["state_sha256", "model", "adam_step"] {
        assert_eq!(
            again["actual_identity"][field],
            result["actual_identity"][field]
        );
    }
    assert_eq!(result["actual_identity"]["state_sha256"], states[1]);
    // The W5 lineage refuses an ordinary continuation.
    let next = ExpandedModelSourceV1 {
        checkpoint: last_checkpoint,
        ..s.clone()
    };
    let error = update(
        &next,
        vec![PinnedFileV1 {
            path: root.join("missing.json"),
            sha256: "0".repeat(64),
        }],
        gae(),
        CheckpointRecordFormatV1::V1,
        root.join("ordinary"),
    )
    .unwrap_err();
    assert!(error.contains("ex1 continuation"), "{error}");
    let _ = fs::remove_dir_all(&root);
}

/// W5 weights-only snapshots of an S checkpoint: the snapshot is under
/// half of the full state and never loads as a train state; its
/// materialization resumes to exactly the full checkpoint's state with zero
/// moments (same parameters, step and policy identity, the state hash of
/// that state); unknown kinds and mismatched templates are refused.
#[test]
fn s_weights_snapshot_materializes_the_full_checkpoint_parameters() {
    on_large_stack(s_weights_snapshot_v1);
}

fn s_weights_snapshot_v1() {
    use super::super::weights_snapshot_v1::{materialize_weights_snapshot_v1, weights_snapshot_v1};
    let root = temp_root("snapshot");
    let net8 = net8_source(&root);
    let s = s_source(&root, &net8);
    let collection = collect(
        &s,
        vec![episode("snap-self", 2_026_110_401, None)],
        root.join("collect"),
    )
    .unwrap();
    let trajectories: Vec<PinnedFileV1> =
        serde_json::from_value(collection["trajectories"].clone()).unwrap();
    let updated = update(
        &s,
        trajectories,
        gae(),
        CheckpointRecordFormatV1::V1,
        root.join("update"),
    )
    .unwrap();
    let full: PinnedFileV1 = serde_json::from_value(updated["checkpoint"].clone()).unwrap();
    let full_source = ExpandedModelSourceV1 {
        checkpoint: Some(full.clone()),
        ..s.clone()
    };
    let receipt = weights_snapshot_v1(&full, &root.join("snapshot.bin")).unwrap();
    assert_eq!(
        receipt["cut_from_state_sha256"],
        updated["after_state_sha256"]
    );
    assert_eq!(receipt["adam_step"], 1);
    let snapshot: PinnedFileV1 = serde_json::from_value(receipt["output"].clone()).unwrap();
    let record: SLearnerCheckpointV1 = read_pinned(&full).unwrap();
    let full_bytes =
        fs::metadata(&full.path).unwrap().len() + fs::metadata(&record.state.path).unwrap().len();
    let snapshot_bytes = fs::metadata(&snapshot.path).unwrap().len();
    assert!(
        snapshot_bytes * 2 < full_bytes,
        "snapshot {snapshot_bytes} bytes against a {full_bytes}-byte full state"
    );
    // Never a train state, never snapshotted again.
    assert!(decode_checkpoint_v1(&fs::read(&snapshot.path).unwrap()).is_err());
    let as_record = ExpandedModelSourceV1 {
        checkpoint: Some(snapshot.clone()),
        ..s.clone()
    };
    assert!(load_inference_v1(&as_record).is_err());
    assert!(weights_snapshot_v1(&snapshot, &root.join("again.bin"))
        .unwrap_err()
        .contains("already a weights snapshot"));
    // An unknown record kind is refused.
    let unknown = root.join("unknown.json");
    let unknown_bytes = br#"{"schema":"w5-s-learner-checkpoint/v0","state":null}"#;
    fs::write(&unknown, unknown_bytes).unwrap();
    assert!(weights_snapshot_v1(
        &PinnedFileV1 {
            path: unknown,
            sha256: sha(unknown_bytes),
        },
        &root.join("unknown-snapshot.bin"),
    )
    .is_err());
    // Materialized: the full checkpoint's state with zero moments.
    let made =
        materialize_weights_snapshot_v1(&s, &snapshot, &root.join("materialized.json")).unwrap();
    assert_eq!(made["parameters_sha256"], receipt["parameters_sha256"]);
    let materialized: PinnedFileV1 = serde_json::from_value(made["output"].clone()).unwrap();
    let materialized_source = ExpandedModelSourceV1 {
        checkpoint: Some(materialized.clone()),
        ..s.clone()
    };
    let resumed = load_s_v1(&full_source, None).unwrap();
    let total = resumed.state.model_v1().layout_v1().total;
    let zeroed = STrainStateV1::from_parts_v1(
        resumed.state.model_v1().clone(),
        vec![0.0; total],
        vec![0.0; total],
        resumed.state.adam_step_v1(),
    )
    .unwrap();
    let from_snapshot = load_s_v1(&materialized_source, None).unwrap();
    assert_eq!(from_snapshot.state, zeroed);
    assert_eq!(
        made["materialized_state_sha256"],
        json!(zeroed.state_sha256_v1())
    );
    assert_ne!(
        made["materialized_state_sha256"],
        updated["after_state_sha256"]
    );
    let (_, full_identity) = load_inference_v1(&full_source).unwrap();
    let (_, made_identity) = load_inference_v1(&materialized_source).unwrap();
    assert_eq!(made_identity.model, full_identity.model);
    assert_eq!(made_identity.adam_step, full_identity.adam_step);
    assert_eq!(made_identity.state_sha256, zeroed.state_sha256_v1());
    // The materialized record is the cut record with the new state.
    let made_record: SLearnerCheckpointV1 = read_pinned(&materialized).unwrap();
    assert_eq!(
        SLearnerCheckpointV1 {
            state: record.state.clone(),
            state_sha256: record.state_sha256.clone(),
            ..made_record.clone()
        },
        record
    );
    // A canonical snapshot whose embedded record names another state or
    // step is refused before any file is written.
    let record_value: Value = read_pinned(&full).unwrap();
    for (index, field, value) in [
        (0, "state_sha256", json!("0".repeat(64))),
        (1, "adam_step", json!(7)),
    ] {
        let mut wrong = record_value.clone();
        wrong[field] = value;
        let bytes = crate::s_transformer_v1::encode_weights_snapshot_v1(&resumed.state, &wrong);
        let path = root.join(format!("inconsistent-{index}.bin"));
        fs::write(&path, &bytes).unwrap();
        let output = root.join(format!("inconsistent-{index}.json"));
        let error = materialize_weights_snapshot_v1(
            &s,
            &PinnedFileV1 {
                path,
                sha256: sha(&bytes),
            },
            &output,
        )
        .unwrap_err();
        assert!(error.contains("another state or step"), "{error}");
        assert!(!output.exists());
        assert!(!root
            .join(format!(
                "inconsistent-{index}.json.{S_LEARNER_STATE_FILE_V1}"
            ))
            .exists());
    }
    // A fresh output is required; a net-8 template refuses the S snapshot.
    assert!(
        materialize_weights_snapshot_v1(&s, &snapshot, &root.join("materialized.json")).is_err()
    );
    assert!(
        materialize_weights_snapshot_v1(&net8, &snapshot, &root.join("net8-made.json")).is_err()
    );
    let _ = fs::remove_dir_all(&root);
}
