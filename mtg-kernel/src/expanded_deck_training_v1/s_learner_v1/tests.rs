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
    let iteration: PinnedFileV1 = serde_json::from_value(first["iterations"][0].clone()).unwrap();
    let iteration: Value = read_pinned(&iteration).unwrap();
    let update: PinnedFileV1 = serde_json::from_value(iteration["update"].clone()).unwrap();
    let update: Value = read_pinned(&update).unwrap();
    assert_eq!(update["numerical_backend"], "native-cpu-exact-parallel-s");
    assert_eq!(update["s_learner_workers_requested"], 2);
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
        record_envelope: Default::default(),
        verified_reads: Default::default(),
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
    // The S container document Python tools read (`checkpoint_container_v1 s-document`).
    let document = super::super::s_container_document_v1(&record.state).unwrap();
    assert_eq!(document["config"], json!(SMALL_V1));
    assert_eq!(document["adam_step"], json!(record.adam_step));
    assert_eq!(document["state_sha256"], json!(record.state_sha256));
    assert_eq!(document["architecture"], json!(record.architecture));
    let layout = SModelV1::init_pilot_scheme_v1(SMALL_V1, 1).unwrap();
    assert_eq!(
        document["parameters"].as_array().unwrap().len(),
        layout.layout_v1().tensors.len()
    );
    assert_eq!(document["parameters"][0]["name"], "ref_bias");
    let initial_document =
        super::super::s_container_document_v1(&s.s_learner.as_ref().unwrap().initial).unwrap();
    assert_eq!(initial_document["parameters"], document["parameters"]);
    assert!(super::super::s_container_document_v1(&full).is_err());
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

/// S search roles skip the state digest, whose features S never reads: the
/// W5 collection (trajectories, search records but their wall times) and the
/// update after it are byte for byte those of the path with the digest on.
#[test]
fn s_search_without_the_state_digest_gives_the_same_records() {
    on_large_stack(s_search_digest_skip_v1);
}

fn s_search_digest_skip_v1() {
    use crate::sideboard_play_policy_v1::S_SEARCH_DIGEST_SKIP_V1;
    let root = temp_root("digest-skip");
    let net8 = net8_source(&root);
    let s = s_source(&root, &net8);
    let (policy, _) = load_inference_v1(&s).unwrap();
    assert!(policy.fork_for_search_v1().unwrap().s_state_digest_off_v1());
    assert!(!policy
        .fork_for_collection_v3()
        .unwrap()
        .s_state_digest_off_v1());
    assert!(!policy.s_state_digest_off_v1());
    let (net8_policy, _) = initialize_any_math_v1(&net8).unwrap();
    assert!(!net8_policy
        .fork_for_search_v1()
        .unwrap()
        .s_state_digest_off_v1());
    // A chain id whose placements fall early in all four episodes, so the
    // searches reach their roots and teach rows.
    let seeds = [
        2_026_110_311u64,
        2_026_110_312,
        2_026_110_313,
        2_026_110_314,
    ];
    let chain =
        (0..1_000_000)
            .map(|k| format!("w5/skip-{k}"))
            .find(|chain| {
                seeds.iter().enumerate().all(|(slot, &seed)| {
                    crate::w5_xact_v1::w5_placement_index_v1(
                        crate::w5_xact_v1::w5_placement_hash_v1(chain, 0, seed, slot as u64),
                    ) <= 16
                })
            })
            .unwrap();
    let run = |skip: bool, label: &str| {
        S_SEARCH_DIGEST_SKIP_V1.with(|flag| flag.set(skip));
        let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
            source: s.clone(),
            episodes: seeds
                .iter()
                .enumerate()
                .map(|(slot, &seed)| {
                    let opponent = (slot % 2 == 1).then(|| net8.clone());
                    episode(&format!("skip-{slot}"), seed, opponent)
                })
                .collect(),
            max_non_natural_episode_fraction: 0.0,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: Some(super::super::ex1_collect_v1::Ex1CollectOptionsV1 {
                run: chain.clone(),
                update: 0,
                budget: 8_000,
                w5: Some(super::super::ex1_collect_v1::W5CollectOptionsV1 {
                    recipe: crate::w5_xact_v1::W5_XACT_VERSION_V1.into(),
                    key_memo_mib: 512,
                    capture_0b: None,
                }),
            }),
            output_directory: root.join(format!("collect-{label}")),
        })
        .unwrap();
        S_SEARCH_DIGEST_SKIP_V1.with(|flag| flag.set(true));
        let trajectories: Vec<PinnedFileV1> =
            serde_json::from_value(collection["trajectories"].clone()).unwrap();
        let hashes: Vec<String> = trajectories.iter().map(|p| p.sha256.clone()).collect();
        let sidecars: Vec<PinnedFileV1> =
            serde_json::from_value(collection["ex1"]["episodes"].clone()).unwrap();
        let records: Vec<Value> = sidecars
            .iter()
            .map(|pin| {
                let mut record: Value = read_pinned(pin).unwrap();
                for attempt in record["attempts"].as_array_mut().unwrap() {
                    for search in attempt["searches"].as_array_mut().unwrap() {
                        search["wall"] = Value::Null;
                    }
                }
                record
            })
            .collect();
        // Successful searches only, so the comparison covers delivered targets.
        let searched: usize = records
            .iter()
            .flat_map(|r| r["attempts"].as_array().unwrap().iter())
            .flat_map(|a| a["searches"].as_array().unwrap().iter())
            .inspect(|search| assert_eq!(search["failed"], false, "a search failed"))
            .count();
        let ex1 = super::super::ex1_update_v1::Ex1UpdateOptionsV1 {
            run: chain.clone(),
            update: 0,
            coefficient: 1.0,
            cap: super::super::search_targets_v1::SearchTargetCapV1::Ratio { kappa: 2.0 },
            collection: PinnedFileV1 {
                path: root
                    .join(format!("collect-{label}"))
                    .join("collection.json"),
                sha256: sha(&fs::read(
                    root.join(format!("collect-{label}"))
                        .join("collection.json"),
                )
                .unwrap()),
            },
            w5: Some(super::super::ex1_update_v1::W5UpdateOptionsV1 {
                recipe: crate::w5_xact_v1::W5_XACT_VERSION_V1.into(),
                budget: 8_000,
                key_memo_mib: 512,
            }),
        };
        let updated = execute_v1(ExpandedTrainingCommandV1::Ex1Update {
            source: s.clone(),
            trajectories,
            learning_rate: 1.0e-3,
            value_coefficient: 0.5,
            loss_selection: gae(),
            ex1,
            output_directory: root.join(format!("update-{label}")),
        })
        .unwrap();
        let ex1_pin: PinnedFileV1 =
            serde_json::from_value(updated["ex1"]["receipt"].clone()).unwrap();
        let mut receipt: Value = read_pinned(&ex1_pin).unwrap();
        assert!(receipt["rows"].as_u64().unwrap() > 0, "no target row");
        assert!(
            !receipt["w5"]["pg_masked"].as_array().unwrap().is_empty(),
            "no search-played row"
        );
        receipt["collection"] = Value::Null;
        receipt["search_wall_seconds"] = Value::Null;
        for episode in receipt["episodes"].as_array_mut().unwrap() {
            episode["record"] = Value::Null;
        }
        (
            hashes,
            records,
            searched,
            updated["after_state_sha256"].clone(),
            receipt,
        )
    };
    let skipped = run(true, "skip");
    let digested = run(false, "digest");
    assert!(skipped.2 > 0, "no search ran");
    assert_eq!(skipped.0, digested.0, "trajectories differ");
    assert_eq!(skipped.1, digested.1, "search records differ");
    assert_eq!(skipped.3, digested.3, "update states differ");
    assert_eq!(skipped.4, digested.4, "ex1 update receipts differ");
    let _ = fs::remove_dir_all(&root);
}

/// Timing receipt (ignored; run by hand on one host): S's W5 searches at
/// 16k on fixed seeds with the state digest on and skipped, interleaved
/// twice (on, off, on, off) against host drift (pilot-size S, random
/// initialization); per-search walls and the records' equality.
#[test]
#[ignore = "timing receipt"]
fn s_search_digest_skip_timing_receipt() {
    on_large_stack(s_search_digest_skip_timing_v1);
}

fn s_search_digest_skip_timing_v1() {
    use crate::sideboard_play_policy_v1::S_SEARCH_DIGEST_SKIP_V1;
    let root = temp_root("digest-timing");
    let net8 = net8_source(&root);
    let model = SModelV1::init_pilot_scheme_v1(SConfigV1::s_v1(), 20_261_104).unwrap();
    let bytes = encode_checkpoint_v1(&STrainStateV1::fresh_optimizer_v1(model));
    let path = root.join("s-pilot.bin");
    fs::write(&path, &bytes).unwrap();
    let s = ExpandedModelSourceV1 {
        s_learner: Some(Box::new(SLearnerSourceV1 {
            schema: S_LEARNER_SOURCE_SCHEMA_V1.into(),
            initial: PinnedFileV1 {
                path,
                sha256: sha(&bytes),
            },
        })),
        ..net8.clone()
    };
    let seeds: Vec<u64> = (0..12).map(|i| 2_026_110_500 + i).collect();
    let chain =
        (0..1_000_000)
            .map(|k| format!("w5/probe-{k}"))
            .find(|chain| {
                seeds.iter().enumerate().all(|(slot, &seed)| {
                    crate::w5_xact_v1::w5_placement_index_v1(
                        crate::w5_xact_v1::w5_placement_hash_v1(chain, 0, seed, slot as u64),
                    ) <= 48
                })
            })
            .unwrap();
    let mut outputs = Vec::new();
    for (round, skip) in [false, true, false, true].into_iter().enumerate() {
        S_SEARCH_DIGEST_SKIP_V1.with(|flag| flag.set(skip));
        let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
            source: s.clone(),
            episodes: seeds
                .iter()
                .map(|&seed| episode(&format!("timing-{seed}"), seed, None))
                .collect(),
            max_non_natural_episode_fraction: 0.0,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: Some(super::super::ex1_collect_v1::Ex1CollectOptionsV1 {
                run: chain.clone(),
                update: 0,
                budget: 16_000,
                w5: Some(super::super::ex1_collect_v1::W5CollectOptionsV1 {
                    recipe: crate::w5_xact_v1::W5_XACT_VERSION_V1.into(),
                    key_memo_mib: 512,
                    capture_0b: None,
                }),
            }),
            output_directory: root.join(format!("collect-{round}")),
        })
        .unwrap();
        S_SEARCH_DIGEST_SKIP_V1.with(|flag| flag.set(true));
        let trajectories: Vec<String> =
            serde_json::from_value::<Vec<PinnedFileV1>>(collection["trajectories"].clone())
                .unwrap()
                .into_iter()
                .map(|p| p.sha256)
                .collect();
        let sidecars: Vec<PinnedFileV1> =
            serde_json::from_value(collection["ex1"]["episodes"].clone()).unwrap();
        let (mut walls, mut records) = (Vec::new(), Vec::new());
        for pin in &sidecars {
            let mut record: Value = read_pinned(pin).unwrap();
            for attempt in record["attempts"].as_array_mut().unwrap() {
                for search in attempt["searches"].as_array_mut().unwrap() {
                    walls.push(search["wall"].as_f64().unwrap());
                    search["wall"] = Value::Null;
                }
            }
            records.push(record);
        }
        eprintln!(
            "TIMING {}",
            json!({"round":round,"state_digest_skipped":skip,"budget":16_000,"walls":walls,
                "collection_elapsed_seconds":collection["collection_elapsed_seconds"]})
        );
        outputs.push((trajectories, records));
    }
    for output in &outputs[1..] {
        assert_eq!(&outputs[0], output, "the skip changed a record");
    }
    let _ = fs::remove_dir_all(&root);
}

/// W5 E5: S sources serve the Spellbench scorer (`spellbench_scorer_v1`)
/// under net-8's contract. The initial state, a full checkpoint record, a
/// materialized weights snapshot and a record under the portable math each
/// load; the ready record names S's identity, state hash and architecture
/// with net-8's feature digests, sampler and float encoding; every choice
/// carries S's own forward on the request tensor, bit for bit (and the
/// policy's in-game forward), with net-8's sampler and completion scoring;
/// a second run gives the same bytes; refusals stay explicit.
#[test]
fn s_sources_serve_the_spellbench_scorer() {
    on_large_stack(s_scorer_v1);
}

/// The scorer's JSON tensor (u32 bits for floats) of a V4 input.
fn scorer_tensor_v1(t: &NativeFlatDecisionTensorV2) -> Value {
    let bits = |values: &[f32]| values.iter().map(|v| v.to_bits()).collect::<Vec<u32>>();
    json!({
        "state": bits(&t.state),
        "object_features": bits(&t.object_features),
        "object_card_ids": t.object_card_ids,
        "object_groups": t.object_groups,
        "object_node_ids": t.object_node_ids,
        "edge_features": bits(&t.edge_features),
        "edge_source_indices": t.edge_source_indices,
        "edge_target_indices": t.edge_target_indices,
        "action_features": bits(&t.action_features),
        "action_ref_features": bits(&t.action_ref_features),
        "action_ref_card_ids": t.action_ref_card_ids,
        "action_ref_action_indices": t.action_ref_action_indices,
        "action_ref_node_indices": t.action_ref_node_indices,
    })
}

/// The scorer's JSONL output records.
fn scorer_records_v1(output: &[u8]) -> Vec<Value> {
    output
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

/// The scorer's output for a config file of `source` over `requests`.
fn run_scorer_v1(
    root: &Path,
    name: &str,
    source: &Value,
    selection: &str,
    requests: &[u8],
) -> Vec<u8> {
    use crate::spellbench_scorer_v1::{
        run_spellbench_scorer_config_path_v1, SCORER_CONFIG_SCHEMA_V1,
    };
    let path = root.join(format!("{name}.scorer.json"));
    let config =
        json!({"schema": SCORER_CONFIG_SCHEMA_V1, "source": source, "selection": selection});
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let mut output = Vec::new();
    let _ = run_spellbench_scorer_config_path_v1(
        &path,
        &mut std::io::Cursor::new(requests.to_vec()),
        &mut output,
    );
    output
}

/// One request line per element, newline-terminated.
fn lines_v1(requests: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for request in requests {
        bytes.extend_from_slice(&serde_json::to_vec(request).unwrap());
        bytes.push(b'\n');
    }
    bytes
}

fn s_scorer_v1() {
    use super::super::weights_snapshot_v1::{materialize_weights_snapshot_v1, weights_snapshot_v1};
    use crate::fast_sampler::WideCategoricalScratchV1;
    use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
    use crate::spellbench_scorer_v1::{
        SCORER_CHOICE_SCHEMA_V1, SCORER_PROPOSAL_SCHEMA_V1, SCORER_READY_SCHEMA_V1,
        SCORER_REQUEST_SCHEMA_V1, SELECTION_ARGMAX_FIRST_V1, SELECTION_SAMPLED_WIDE_V1,
    };
    use sha2::{Digest, Sha256};

    let root = temp_root("scorer");
    let net8 = net8_source(&root);
    let s = s_source(&root, &net8);
    let collection = collect(
        &s,
        vec![episode("scorer-self", 2_026_110_501, None)],
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
    let receipt = weights_snapshot_v1(&full, &root.join("snapshot.bin")).unwrap();
    let snapshot: PinnedFileV1 = serde_json::from_value(receipt["output"].clone()).unwrap();
    let made =
        materialize_weights_snapshot_v1(&s, &snapshot, &root.join("materialized.json")).unwrap();
    let materialized: PinnedFileV1 = serde_json::from_value(made["output"].clone()).unwrap();
    // The same record as if its net-8 opponents had played under the portable
    // math: S's forward reads no training math, so it serves.
    let mut portable_record: Value = read_pinned(&full).unwrap();
    portable_record["training_math"] = json!("portable-math-v1");
    let portable_bytes = serde_json::to_vec(&portable_record).unwrap();
    let portable_path = root.join("portable.json");
    fs::write(&portable_path, &portable_bytes).unwrap();
    let portable = PinnedFileV1 {
        path: portable_path,
        sha256: sha(&portable_bytes),
    };
    let with = |checkpoint: Option<&PinnedFileV1>| ExpandedModelSourceV1 {
        checkpoint: checkpoint.cloned(),
        ..s.clone()
    };
    let sources = [
        ("initial", with(None)),
        ("full", with(Some(&full))),
        ("materialized", with(Some(&materialized))),
        ("portable", with(Some(&portable))),
    ];

    // Live Rally decisions with at least two candidates.
    let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        4242,
        17,
        400,
        40_000,
        ["Rally".to_string(), "Rally".to_string()],
    )
    .unwrap();
    let mut sessions = Vec::new();
    while sessions.len() < 6 {
        let FastActorResponseV1::Decision(decision) = session.current_response() else {
            panic!("game ended")
        };
        if decision.legal_action_count >= 2 {
            sessions.push(session.clone());
        }
        session.step(decision.episode_id, decision.step, 0).unwrap();
    }

    // Net-8's ready record on the same shell: every field but the model's own
    // identity is the contract the bot pins.
    let net8_ready = scorer_records_v1(&run_scorer_v1(
        &root,
        "net8",
        &serde_json::to_value(&net8).unwrap(),
        SELECTION_SAMPLED_WIDE_V1,
        b"",
    ))
    .remove(0);
    assert_eq!(net8_ready["schema"], SCORER_READY_SCHEMA_V1);
    assert!(net8_ready.get("model_architecture").is_none());
    let contract_of = |ready: &Value| {
        let mut contract = ready.clone();
        let fields = contract.as_object_mut().unwrap();
        for field in ["model", "model_state_sha256", "model_architecture"] {
            fields.remove(field);
        }
        contract
    };

    let mut states = BTreeSet::new();
    for (name, source) in &sources {
        let source_value = serde_json::to_value(source).unwrap();
        let (mut policy, identity) = super::super::load_expanded_inference_v1(source)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let model = policy.s_model_v1().unwrap().clone();
        let mut requests = Vec::new();
        let mut expected = Vec::new();
        for (index, live) in sessions.iter().enumerate() {
            let scores = policy.score_fast_session_v1(live).unwrap();
            let tensor = policy
                .last_scored_training_tensor_v4()
                .unwrap()
                .common
                .clone();
            let (logits, value) = model.score_tensor_v1(&tensor).unwrap();
            // S's forward on the request tensor is the policy's in-game one.
            let bits = |xs: &[f32]| xs.iter().map(|x| x.to_bits()).collect::<Vec<u32>>();
            assert_eq!(bits(&logits), bits(&scores.logits), "{name}");
            assert_eq!(value.to_bits(), scores.value.to_bits(), "{name}");
            let rows: Vec<u32> = (0..logits.len() as u32).map(|row| 100 + row).collect();
            let seed = 77 + index as u64;
            requests.push(json!({
                "schema": SCORER_REQUEST_SCHEMA_V1,
                "request_id": format!("g0:{index}"), "game_id": "g0",
                "seat": "p0", "step": index,
                "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
                "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4,
                "row_candidate_ids": rows, "sample_seed": seed,
                "tensor": scorer_tensor_v1(&tensor),
            }));
            expected.push((logits, value, rows, seed, tensor));
        }
        let bytes = lines_v1(&requests);
        let output = run_scorer_v1(
            &root,
            name,
            &source_value,
            SELECTION_SAMPLED_WIDE_V1,
            &bytes,
        );
        // Deterministic: a second load and run gives the same bytes.
        assert_eq!(
            run_scorer_v1(
                &root,
                &format!("{name}-again"),
                &source_value,
                SELECTION_SAMPLED_WIDE_V1,
                &bytes
            ),
            output,
            "{name}"
        );
        let records = scorer_records_v1(&output);
        assert_eq!(records.len(), 1 + expected.len(), "{name}: {records:?}");
        let ready = &records[0];
        assert_eq!(ready["model_state_sha256"], json!(identity.state_sha256));
        assert_eq!(ready["model"], serde_json::to_value(&identity).unwrap());
        assert_eq!(
            ready["model"]["model"]["weights_sha256"],
            json!(model.parameter_sha256_v1())
        );
        assert_eq!(ready["model_architecture"], S_ARCHITECTURE_V1, "{name}");
        assert_eq!(contract_of(ready), contract_of(&net8_ready), "{name}");
        assert_eq!(ready["feature_contract_digest"], FEATURE_CONTRACT_DIGEST_V4);
        assert_eq!(ready["feature_encoding_digest"], FEATURE_ENCODING_DIGEST_V4);
        states.insert(identity.state_sha256.clone());
        for (index, (choice, (logits, value, rows, seed, _))) in
            records[1..].iter().zip(&expected).enumerate()
        {
            let line = serde_json::to_vec(&requests[index]).unwrap();
            assert_eq!(choice["schema"], SCORER_CHOICE_SCHEMA_V1);
            assert_eq!(
                choice["request_sha256"],
                format!("{:x}", Sha256::digest(&line))
            );
            assert_eq!(choice["model_state_sha256"], json!(identity.state_sha256));
            assert_eq!(
                choice["logits_bits"],
                json!(logits.iter().map(|x| x.to_bits()).collect::<Vec<_>>()),
                "{name}"
            );
            assert_eq!(choice["value_bits"], json!(value.to_bits()), "{name}");
            let row = WideCategoricalScratchV1::default()
                .sample(logits, *seed)
                .unwrap();
            assert_eq!(choice["selected_row"], json!(row));
            assert_eq!(choice["selected_candidate_id"], json!(rows[row]));
        }
        // Argmax selection: the first maximum of S's logits.
        let unseeded: Vec<Value> = requests
            .iter()
            .map(|request| {
                let mut request = request.clone();
                request["sample_seed"] = Value::Null;
                request
            })
            .collect();
        let argmax = scorer_records_v1(&run_scorer_v1(
            &root,
            &format!("{name}-argmax"),
            &source_value,
            SELECTION_ARGMAX_FIRST_V1,
            &lines_v1(&unseeded),
        ));
        assert_eq!(argmax.len(), 1 + expected.len(), "{name}");
        for (choice, (logits, ..)) in argmax[1..].iter().zip(&expected) {
            let first = (0..logits.len()).fold(0, |best, row| {
                if logits[row] > logits[best] {
                    row
                } else {
                    best
                }
            });
            assert_eq!(choice["selected_row"], json!(first), "{name}");
        }
        // Completion proposals: each candidate's one-step completion scored
        // by S's log probability of its row, then net-8's sampler.
        let (logits, _, _, _, tensor) = &expected[0];
        let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
        let total = logits
            .iter()
            .map(|x| (*x as f64 - maximum).exp())
            .sum::<f64>()
            .ln();
        let log_probabilities: Vec<f32> = logits
            .iter()
            .map(|x| (*x as f64 - maximum - total) as f32)
            .collect();
        let proposals: Vec<Value> = (0..logits.len())
            .map(|row| json!([{"tensor": scorer_tensor_v1(tensor), "selected_row": row}]))
            .collect();
        let rows: Vec<u32> = (0..logits.len() as u32).collect();
        let proposal = json!({
            "schema": SCORER_PROPOSAL_SCHEMA_V1, "request_id": "proposal-1",
            "game_id": "g0", "seat": "p1", "step": 0,
            "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V4,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V4,
            "sample_seed": 123, "row_candidate_ids": rows, "proposals": proposals,
        });
        let answered = scorer_records_v1(&run_scorer_v1(
            &root,
            &format!("{name}-proposal"),
            &source_value,
            SELECTION_SAMPLED_WIDE_V1,
            &lines_v1(&[proposal]),
        ));
        assert_eq!(answered.len(), 2, "{name}: {answered:?}");
        assert_eq!(
            answered[1]["mapping"],
            "deterministic-completion-logprob/v1"
        );
        assert_eq!(
            answered[1]["logits_bits"],
            json!(log_probabilities
                .iter()
                .map(|x| x.to_bits())
                .collect::<Vec<_>>()),
            "{name}"
        );
        let row = WideCategoricalScratchV1::default()
            .sample(&log_probabilities, 123)
            .unwrap();
        assert_eq!(answered[1]["selected_row"], json!(row), "{name}");
    }
    // The initial, full and materialized states are three states; the
    // portable record names the full state.
    assert_eq!(states.len(), 3);

    // Refusals stay explicit, one error record each: an S source on another
    // feature contract, an initial state that is not the pinned one, and an
    // S weights snapshot pinned as a checkpoint record.
    let mut other_contract = serde_json::to_value(&sources[1].1).unwrap();
    other_contract["feature_transfer"]["expected_feature_contract_digest"] = json!("0".repeat(64));
    let mut wrong_pin = serde_json::to_value(&sources[0].1).unwrap();
    wrong_pin["s_learner"]["initial"]["sha256"] = json!("0".repeat(64));
    let as_record = serde_json::to_value(with(Some(&snapshot))).unwrap();
    for (name, source) in [
        ("other-contract", other_contract),
        ("wrong-pin", wrong_pin),
        ("snapshot", as_record),
    ] {
        let records = scorer_records_v1(&run_scorer_v1(
            &root,
            name,
            &source,
            SELECTION_SAMPLED_WIDE_V1,
            b"",
        ));
        assert_eq!(
            records,
            vec![json!({
                "schema": "mtg-kernel-spellbench-scorer-error/v1",
                "code": "checkpoint_load",
                "service_continues": false,
            })],
            "{name}"
        );
    }
    let _ = fs::remove_dir_all(&root);
}
