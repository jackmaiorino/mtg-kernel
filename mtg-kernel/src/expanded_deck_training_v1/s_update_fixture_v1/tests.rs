//! The saved-batch fixture: retained initial and warmed batches, exact
//! benches on every path, commands for older builds, verification,
//! root independence, and strict arguments.

use super::*;
use crate::expanded_deck_training_v1::s_learner_v1::{
    execute_s_update_v1, SUnsupportedOptionsV1, SUpdateInputsV1, VerifiedReplayAttemptV1,
    VerifiedReplayOptionsV1,
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
        "s-update-fixture-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn fixture_serial_inputs_v1(
    manifest: &FixtureManifestV1,
    batch: &FixtureBatchV1,
    output_directory: PathBuf,
    verified_tape_cap: Option<usize>,
) -> SUpdateInputsV1 {
    SUpdateInputsV1 {
        source: batch.source.clone(),
        trajectories: batch.trajectories.clone(),
        learning_rate: manifest.learning.learning_rate,
        value_coefficient: manifest.learning.value_coefficient,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        backward_execution: UpdateBackwardExecutionV1::Sequential,
        loss_selection: manifest.learning.loss_selection,
        output_directory,
        preparation_workers: None,
        max_prepared_tensor_mebibytes: manifest.options.max_prepared_tensor_mebibytes,
        verified_replay: verified_tape_cap.map(|cap| VerifiedReplayOptionsV1::new(cap).unwrap()),
        unsupported: SUnsupportedOptionsV1 {
            line_b: false,
            e_supported: false,
            soft_q: false,
            search_targets: false,
        },
        ex1: None,
        training_math: TrainingMathV1::PlatformLibm,
        record_format: CheckpointRecordFormatV1::default(),
        started: Instant::now(),
    }
}

fn fixture_verified_cap_v1(batch: &FixtureBatchV1) -> usize {
    // The census counts live payload. Include the slot's headers and leave
    // headroom for ragged retained capacities.
    usize::try_from(batch.census.learner_row_tape_bytes.max)
        .expect("fixture tape bytes fit usize")
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(4096))
        .expect("fixture tape cap fits usize")
}

fn assert_verified_replay_bench_v1(
    root: &Path,
    fixture: &Path,
    batch: &FixtureBatchV1,
    reference_profile: &Value,
) {
    let cap = fixture_verified_cap_v1(batch);
    let receipt = main_v1(&args(&[
        "bench",
        &format!("fixture={}", fixture.display()),
        &format!("batch={}", batch.name),
        "path=serial",
        &format!("verified_replay_tape_bytes={cap}"),
        &format!(
            "out={}",
            root.join(format!("verified-{}", batch.name)).display()
        ),
    ]))
    .unwrap();
    let sample = &receipt["samples"][0];
    assert_eq!(sample["verified"], true, "{sample}");
    let replay = &sample["verified_replay"];
    assert_eq!(replay["used"], true, "{replay}");
    let peak = replay["attempt"]["peak_retained_tape_bytes"].as_u64().unwrap();
    assert!(peak > batch.census.learner_row_tape_bytes.max);
    assert!(peak <= u64::try_from(cap).unwrap());
    assert_eq!(
        replay["attempt"]["learner_replay_forwards"],
        batch.census.learner_rows
    );
    assert_eq!(
        replay["attempt"]["other_replay_forwards"],
        batch.census.other_rows
    );
    assert_eq!(
        replay["attempt"]["learner_backwards"],
        batch.census.learner_rows
    );
    assert_combined_seconds_v1(sample, false);
    if s_update_profile::ENABLED_V1 {
        let profile = &sample["forward_profile"];
        assert_eq!(sample["attribution_verified"], true);
        assert_eq!(profile["counts_match_execution"], true);
        assert_eq!(profile["forward_calls"], batch.census.rows);
        assert_eq!(
            profile["removed_learner_forwards"],
            batch.census.learner_rows
        );
        assert_eq!(profile["stages"]["learner_forward"]["calls"], 0);
        for stage in ["replay_learner_rows", "replay_other_rows"] {
            assert_eq!(
                profile["stages"][stage]["output_digest"],
                reference_profile["stages"][stage]["output_digest"]
            );
        }
    } else {
        assert!(sample["attribution_verified"].is_null());
    }
}

fn assert_combined_seconds_v1(result: &Value, fallback: bool) {
    let replay = &result["verified_replay"];
    let attempt = replay["replay_and_gradient_seconds"].as_f64().unwrap();
    let reference_replay = result["behavior_replay_seconds"].as_f64().unwrap();
    let learner = result["learner_update_seconds"].as_f64().unwrap();
    assert_eq!(
        replay["combined_replay_learner_seconds"].as_f64().unwrap(),
        reference_replay + learner + if fallback { attempt } else { 0.0 }
    );
}

fn assert_verified_replay_refusals_v1(
    root: &Path,
    manifest: &FixtureManifestV1,
    batch: &FixtureBatchV1,
) {
    let fallback = execute_v1(
        update_command_v1(
            manifest,
            batch,
            UpdatePathV1::VerifiedSerial { tape_cap_bytes: 1 },
            1,
            root.join("verified-fallback"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(fallback["verified_replay"]["used"], false);
    assert_eq!(fallback["verified_replay"]["fallback"], "tape_capacity");
    assert_combined_seconds_v1(&fallback, true);
    assert_eq!(
        verify_v1(manifest, batch, &fallback).unwrap()["verified"],
        true
    );

    let cap = fixture_verified_cap_v1(batch);
    let out = root.join("refused-backward");
    let mut inputs = fixture_serial_inputs_v1(manifest, batch, out.clone(), Some(cap));
    inputs
        .verified_replay
        .as_mut()
        .unwrap()
        .short_first_gradient = true;
    assert_eq!(
        execute_s_update_v1(inputs).unwrap_err(),
        "S gradient buffer has the wrong length"
    );
    assert!(!out.exists());

    for (pairing, message) in [
        (false, "stored tensor does not reproduce rollout outputs"),
        (true, "only a FlatV8 policy replays a pairing relation"),
    ] {
        let mut trajectory: ExpandedTrajectoryV1 = read_pinned(&batch.trajectories[1]).unwrap();
        let seat = trajectory.episode.learner_seat;
        let row = trajectory
            .decisions
            .iter_mut()
            .rev()
            .find(|row| (row.actor == seat) == pairing)
            .unwrap();
        if pairing {
            row.tensor.pairing_node_indices.extend([0, 1]);
        } else {
            // A stored value mismatch leaves the action-sampler check intact.
            row.value ^= 1;
        }
        let path = root.join(format!("corrupt-{pairing}.json"));
        fs::write(&path, serde_json::to_vec(&trajectory).unwrap()).unwrap();
        let pin = pin_file_v1(&path).unwrap();
        let mut corrupt = batch.clone();
        corrupt.trajectories[1] = pin.clone();
        for path in [
            UpdatePathV1::Serial,
            UpdatePathV1::VerifiedSerial { tape_cap_bytes: cap },
        ] {
            let out = root.join(format!("refused-command-{pairing}-{path:?}"));
            let command = update_command_v1(manifest, &corrupt, path, 1, out.clone()).unwrap();
            assert_eq!(execute_v1(command).unwrap_err(), message);
            assert!(!out.exists());
        }
        for opt_in in [None, Some(cap)] {
            let out = root.join(format!("refused-{pairing}-{opt_in:?}"));
            let mut inputs = fixture_serial_inputs_v1(manifest, batch, out.clone(), opt_in);
            inputs.trajectories[1] = pin.clone();
            // Invalid targets must not mask a later replay error.
            inputs.loss_selection = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
                gamma: -0.25,
                lambda: 1.0,
                entropy_coefficient: 0.0,
            };
            assert_eq!(execute_s_update_v1(inputs).unwrap_err(), message);
            assert!(!out.exists());
        }
        if !pairing {
            let out = root.join("refused-backward-then-opponent");
            let mut inputs = fixture_serial_inputs_v1(manifest, batch, out.clone(), Some(cap));
            inputs.trajectories[1] = pin;
            inputs
                .verified_replay
                .as_mut()
                .unwrap()
                .short_first_gradient = true;
            assert_eq!(execute_s_update_v1(inputs).unwrap_err(), message);
            assert!(!out.exists());
        }
    }
}

fn assert_late_capacity_fallback_v1(
    root: &Path,
    manifest: &FixtureManifestV1,
    batch: &FixtureBatchV1,
) {
    use crate::s_transformer_v1::OBJECT_FEATURE_DIM_V1;

    let cap = fixture_verified_cap_v1(batch);
    let first: ExpandedTrajectoryV1 = read_pinned(&batch.trajectories[0]).unwrap();
    let mut trajectory: ExpandedTrajectoryV1 = read_pinned(&batch.trajectories[1]).unwrap();
    let seat = trajectory.episode.learner_seat;
    let index = trajectory
        .decisions
        .iter()
        .rposition(|row| row.actor == seat)
        .unwrap();
    // This fixture uses the legacy sampler, one independent RNG draw per
    // actor row. Preserve the draw while changing only the late row's shape.
    assert!(trajectory.learner_sampler.is_none());
    let seat_rows = trajectory.decisions[..=index]
        .iter()
        .filter(|row| row.actor == seat)
        .count();
    let candidate_backwards = first
        .decisions
        .iter()
        .filter(|row| row.actor == first.episode.learner_seat)
        .count()
        + seat_rows
        - 1;
    assert!(candidate_backwards > 0);
    let mut rng = SplitMix64::seed(paired_policy_seeds_v1(trajectory.episode.seed)[seat as usize]);
    let draw = (0..seat_rows).map(|_| rng.next_u64()).last().unwrap();
    let row = &mut trajectory.decisions[index];
    let config = manifest.config;
    // Attention buffers alone exceed the cap after this enlargement.
    let objects =
        (cap / (std::mem::size_of::<f32>() * config.heads * (1 + config.layers))).isqrt() + 1;
    assert!(objects > row.tensor.object_card_ids.len());
    row.tensor
        .object_features
        .resize(objects * OBJECT_FEATURE_DIM_V1, 0);
    row.tensor.object_card_ids.resize(objects, 0);
    row.tensor.object_groups.resize(objects, 0);
    row.tensor.object_node_ids.resize(objects, 0);
    let input = row.tensor.tensor();
    let counts = SInputV1::of_tensor_v1(&input).validate_v1(&config).unwrap();
    assert!(STapeV1::bytes_for_counts_v1(&config, counts) > cap);
    let (policy, _) = load_expanded_inference_any_math_v1(&batch.source).unwrap();
    let (logits, value) = policy
        .s_model_v1()
        .unwrap()
        .score_tensor_v1(&input)
        .unwrap();
    assert_eq!(logits.len(), row.logits.len());
    row.selected = WideCategoricalScratchV1::default()
        .sample(&logits, draw)
        .unwrap() as u32;
    row.logits = bits(&logits);
    row.value = value.to_bits();

    let mut ragged = batch.clone();
    let path = root.join("late-oversized-trajectory.json");
    fs::write(&path, serde_json::to_vec(&trajectory).unwrap()).unwrap();
    ragged.trajectories[1] = pin_file_v1(&path).unwrap();
    let (census, games_sha256) = census_v1(
        &ragged.trajectories,
        &[first.episode, trajectory.episode],
        &ragged.opponents,
        &config,
        &manifest.options,
    )
    .unwrap();
    ragged.census = census;
    let reference_dir = root.join("late-oversized-reference");
    let reference = execute_s_update_v1(fixture_serial_inputs_v1(
        manifest,
        &ragged,
        reference_dir.clone(),
        None,
    ))
    .unwrap();
    ragged.reference = pin_file_v1(&reference_dir.join("update.json")).unwrap();
    ragged.identity = identity_of_update_v1(&reference, games_sha256).unwrap();

    let profile = s_update_profile::arm_v1();
    let fallback = execute_v1(
        update_command_v1(
            manifest,
            &ragged,
            UpdatePathV1::VerifiedSerial { tape_cap_bytes: cap },
            1,
            root.join("late-oversized-fallback"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(fallback["verified_replay"]["used"], false);
    assert_eq!(fallback["verified_replay"]["fallback"], "tape_capacity");
    assert_combined_seconds_v1(&fallback, true);
    assert_eq!(
        verify_v1(manifest, &ragged, &fallback).unwrap()["verified"],
        true
    );
    let attempt: VerifiedReplayAttemptV1 =
        serde_json::from_value(fallback["verified_replay"]["attempt"].clone()).unwrap();
    assert_eq!(attempt.learner_replay_forwards, candidate_backwards as u64);
    assert_eq!(attempt.learner_backwards, candidate_backwards as u64);
    assert_eq!(
        attempt.other_replay_forwards,
        (first.decisions.len() + index - candidate_backwards) as u64
    );
    assert!((1..=cap).contains(&attempt.peak_retained_tape_bytes));
    let forward = forward_profile_v1(
        profile.totals_v1(),
        &ragged.census,
        Some((false, &attempt)),
    );
    if s_update_profile::ENABLED_V1 {
        assert_eq!(forward["counts_match_execution"], true, "{forward}");
        assert_eq!(forward["removed_learner_forwards"], 0);
        assert_eq!(
            forward["forward_calls"],
            ragged.census.rows as u64 + ragged.census.learner_rows as u64
                + attempt.learner_replay_forwards
                + attempt.other_replay_forwards
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_dirty_mxcsr_fallback_v1(
    root: &Path,
    manifest: &FixtureManifestV1,
    batch: &FixtureBatchV1,
) {
    use crate::deterministic_math_v1::{
        read_mxcsr_v1, verify_pinned_mxcsr_state_v1, write_mxcsr_v1,
    };

    let inputs = fixture_serial_inputs_v1(
        manifest,
        batch,
        root.join("dirty-mxcsr-fallback"),
        Some(fixture_verified_cap_v1(batch)),
    );
    // Use a fresh thread so the reference's first S forward may normalize
    // its MXCSR, as it would at the entry point of a standalone update.
    let fallback = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            const FTZ: u32 = 1 << 15;
            write_mxcsr_v1(read_mxcsr_v1() | FTZ);
            assert!(verify_pinned_mxcsr_state_v1().is_err());
            execute_s_update_v1(inputs).unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(fallback["verified_replay"]["used"], false);
    assert_eq!(fallback["verified_replay"]["fallback"], "caller_math_state");
    assert_eq!(
        verify_v1(manifest, batch, &fallback).unwrap()["verified"],
        true
    );
}

#[test]
fn s_update_fixture_retains_batches_and_benches_every_path_exactly() {
    on_large_stack(fixture_end_to_end_v1);
}

fn fixture_end_to_end_v1() {
    let root = temp_root("e2e");
    let fixture = root.join("fixture");
    let options = FixtureOptionsV1::default();
    let manifest = generate_v1(&options, &fixture).unwrap();
    assert_eq!(load_manifest_v1(&fixture).unwrap(), manifest);
    assert_eq!(manifest.config, TEST_CONFIG_V1);
    let [initial, warmed] = &manifest.batches[..] else {
        panic!("two batches");
    };
    assert_eq!(initial.name, "initial");
    assert_eq!(warmed.name, "warmed");
    assert_eq!(initial.opponents, [OpponentV1::Current, OpponentV1::Net8]);
    assert_eq!(warmed.opponents, [OpponentV1::Current, OpponentV1::Initial]);

    // The initial batch starts fresh: step 0 and zero moments, although the
    // initial container carries a step and moments.
    assert!(initial.source.checkpoint.is_none());
    let (_, fresh) = load_expanded_inference_any_math_v1(&initial.source).unwrap();
    assert_eq!(fresh.adam_step, 0);
    assert_eq!(initial.identity.before_state_sha256, fresh.state_sha256);
    assert_eq!(initial.identity.adam_step, 1);
    // The warmed batch starts where the initial reference ends: step 1 and
    // nonzero moments.
    let reference: Value = read_pinned(&initial.reference).unwrap();
    let checkpoint: PinnedFileV1 = serde_json::from_value(reference["checkpoint"].clone()).unwrap();
    assert_eq!(warmed.source.checkpoint, Some(checkpoint));
    assert_eq!(
        warmed.identity.before_state_sha256,
        initial.identity.after_state_sha256
    );
    assert_eq!(warmed.identity.adam_step, 2);
    let container: PinnedFileV1 =
        serde_json::from_value(reference["s_learner"]["state"].clone()).unwrap();
    let warm = decode_checkpoint_v1(&read_pinned_bytes(&container).unwrap()).unwrap();
    assert_eq!(warm.adam_step_v1(), 1);
    assert!(warm.first_moments_v1().iter().any(|m| *m != 0.0));
    assert!(warm.second_moments_v1().iter().any(|m| *m != 0.0));
    assert_eq!(warm.state_sha256_v1(), warmed.identity.before_state_sha256);

    // The census accounts for every row and agrees with the reference.
    for (batch, opponents) in [
        (initial, ["current", "net8"]),
        (warmed, ["current", "initial"]),
    ] {
        let census = &batch.census;
        assert_eq!(census.episodes, options.episodes);
        assert_eq!(census.rows, census.learner_rows + census.other_rows);
        assert!(census.learner_rows > 0 && census.other_rows > 0);
        assert_eq!(
            census.other_rows_by_opponent.values().sum::<usize>(),
            census.other_rows
        );
        assert_eq!(
            census.other_rows_by_opponent.keys().collect::<Vec<_>>(),
            opponents
        );
        assert_eq!(
            census.learner_groups as u64,
            batch.identity.physical_decisions
        );
        assert_eq!(census.learner_rows as u64, batch.identity.policy_substeps);
        assert_eq!(
            census.learner_groups_by_substeps.values().sum::<usize>(),
            census.learner_groups
        );
        assert!(census.learner_row_tape_bytes.min > 0);
        assert!(census.learner_row_tape_bytes.max >= census.learner_row_tape_bytes.min);
        assert!(census.learner_tape_bytes_total >= census.learner_row_tape_bytes.max);
        assert!(census.trajectory_bytes > 0 && census.trajectory_bytes <= MAX_BATCH_BYTES);
        assert!(census.decoded_tensor_payload_bytes > 0);
    }

    // Every path reproduces each reference exactly; the forward counts show
    // each row replayed once and each learner row forwarded once more, with
    // the replay's outputs.
    let executable_sha256 = sha(&fs::read(std::env::current_exe().unwrap()).unwrap());
    for batch in [initial, warmed] {
        for (path, workers) in [
            (UpdatePathV1::Serial, 1),
            (UpdatePathV1::Prepared, 1),
            (UpdatePathV1::Prepared, 2),
        ] {
            let out = root.join(format!("bench-{}-{path:?}-{workers}", batch.name));
            let options = BenchOptionsV1 {
                batch: batch.name.clone(),
                path,
                workers,
                reps: 1,
            };
            let receipt = bench_v1(&fixture, &options, &out).unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(out.join("bench.json")).unwrap())
                    .unwrap(),
                receipt
            );
            assert_eq!(receipt["executable_sha256"], executable_sha256);
            let sample = &receipt["samples"][0];
            assert_eq!(sample["verified"], true, "{sample}");
            assert!(sample.get("verified_replay").is_none());
            assert!(sample["elapsed_seconds"].as_f64().unwrap() > 0.0);
            let profile = &sample["forward_profile"];
            if s_update_profile::ENABLED_V1 {
                assert_eq!(sample["attribution_verified"], true);
                assert_eq!(profile["counts_match_census"], true, "{profile}");
                assert_eq!(
                    profile["learner_forwards_repeat_replayed_rows"], true,
                    "{profile}"
                );
                assert_eq!(profile["duplicate_forwards"], batch.census.learner_rows);
                assert_eq!(
                    profile["forward_calls"],
                    batch.census.rows + batch.census.learner_rows
                );
            } else {
                assert_eq!(profile["enabled"], false);
                assert!(sample["attribution_verified"].is_null());
            }
            if path == UpdatePathV1::Serial {
                assert_verified_replay_bench_v1(&root, &fixture, batch, profile);
            }
        }
    }
    assert_verified_replay_refusals_v1(&root, &manifest, initial);
    assert_late_capacity_fallback_v1(&root, &manifest, warmed);
    #[cfg(target_arch = "x86_64")]
    assert_dirty_mxcsr_fallback_v1(&root, &manifest, initial);
    // A bench refuses an existing directory and a serial path with workers.
    let existing = BenchOptionsV1 {
        batch: "warmed".into(),
        path: UpdatePathV1::Prepared,
        workers: 2,
        reps: 1,
    };
    assert!(
        bench_v1(&fixture, &existing, &root.join("bench-warmed-Prepared-2"))
            .unwrap_err()
            .contains("already exists")
    );
    let serial_with_workers = BenchOptionsV1 {
        path: UpdatePathV1::Serial,
        ..existing.clone()
    };
    assert!(
        bench_v1(&fixture, &serial_with_workers, &root.join("never"))
            .unwrap_err()
            .contains("path=serial runs one worker")
    );
    assert!(!root.join("never").exists());
    assert_verified_command_v1(&root, &fixture, &manifest, warmed);

    // The command for any build's binary carries only fields an older build
    // knows, reads back to the same values, and reproduces the reference.
    let fixture_argument = format!("fixture={}", fixture.display());
    let by_command = format!("out={}", root.join("by-command").display());
    let printed = main_v1(&args(&[
        "command",
        fixture_argument.as_str(),
        "batch=warmed",
        "path=prepared",
        "workers=2",
        by_command.as_str(),
    ]))
    .unwrap();
    let mut keys: Vec<&str> = printed
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "learning_rate",
            "loss_selection",
            "mode",
            "output_directory",
            "preparation_workers",
            "source",
            "trajectories",
            "value_coefficient"
        ]
    );
    let command: ExpandedTrainingCommandV1 = serde_json::from_value(printed).unwrap();
    match &command {
        ExpandedTrainingCommandV1::UpdatePrepared {
            learning_rate,
            value_coefficient,
            preparation_workers,
            ..
        } => {
            assert_eq!(learning_rate.to_bits(), LEARNING_RATE_V1.to_bits());
            assert_eq!(value_coefficient.to_bits(), VALUE_COEFFICIENT_V1.to_bits());
            assert_eq!(*preparation_workers, 2);
        }
        _ => panic!("a prepared command"),
    }
    execute_v1(command).unwrap();
    let result_argument = format!(
        "result={}",
        root.join("by-command").join("update.json").display()
    );
    let verified = main_v1(&args(&[
        "verify",
        fixture_argument.as_str(),
        "batch=warmed",
        result_argument.as_str(),
    ]))
    .unwrap();
    assert_eq!(verified["verified"], true);
    // Another batch's update is not this batch's: the differences are listed.
    let other: Value = read_pinned(&initial.reference).unwrap();
    let report = verify_v1(&manifest, warmed, &other).unwrap();
    assert_eq!(report["verified"], false);
    let mismatches = report["mismatches"].as_array().unwrap();
    for field in [
        "trajectories",
        "after_state_sha256",
        "after_container_sha256",
        "checkpoint_record",
        "recovery",
    ] {
        assert!(mismatches.iter().any(|m| m == field), "{report}");
    }
    let other_argument = format!("result={}", initial.reference.path.display());
    assert!(main_v1(&args(&[
        "verify",
        fixture_argument.as_str(),
        "batch=warmed",
        other_argument.as_str(),
    ]))
    .unwrap_err()
    .contains("differs from the reference"));

    // A signed-zero loss difference is still a different numerical receipt.
    let mut signed_zero = warmed.clone();
    signed_zero.identity.loss = 0.0;
    let mut result: Value = read_pinned(&warmed.reference).unwrap();
    result["loss"] = json!(-0.0);
    let report = verify_v1(&manifest, &signed_zero, &result).unwrap();
    assert_eq!(report["mismatches"], json!(["loss"]));

    // Census edits do not alter the root-independent output identity. The
    // timed path must independently reject them before publishing anything.
    let manifest_file = fixture.join(MANIFEST_FILE_V1);
    let mut forged = manifest.clone();
    forged.batches[1].census.learner_rows += 1;
    fs::write(&manifest_file, serde_json::to_vec(&forged).unwrap()).unwrap();
    let refused = root.join("forged-census");
    assert!(bench_v1(&fixture, &existing, &refused)
        .unwrap_err()
        .contains("fixture census or games identity differs"));
    assert!(!refused.exists());

    // Substituting an actual pin from the wrong file is also rejected.
    forged = manifest.clone();
    let other_pin = forged.batches[0].trajectories[0].clone();
    forged.batches[1].trajectories[0] = other_pin;
    fs::write(&manifest_file, serde_json::to_vec(&forged).unwrap()).unwrap();
    assert!(load_manifest_v1(&fixture)
        .unwrap_err()
        .contains("fixture pin does not name its generated file"));
    fs::write(&manifest_file, serde_json::to_vec(&manifest).unwrap()).unwrap();

    // A second generation elsewhere has the same root-independent identity;
    // a moved fixture is refused.
    let again = generate_v1(&options, &root.join("again")).unwrap();
    assert_eq!(again.identity_sha256, manifest.identity_sha256);
    assert_eq!(again.batches[1].identity, warmed.identity);
    assert_ne!(again.batches[1].trajectories, warmed.trajectories);
    fs::rename(root.join("again"), root.join("moved")).unwrap();
    assert!(load_manifest_v1(&root.join("moved"))
        .unwrap_err()
        .contains("generate it again here"));
    assert!(generate_v1(&options, &fixture)
        .unwrap_err()
        .contains("already exists"));
    // The batch limits are checked after collection.
    let limit = initial.census.trajectory_bytes / MEBIBYTE_V1;
    assert!(limit >= 1, "{}", initial.census.trajectory_bytes);
    let bounded = FixtureOptionsV1 {
        max_batch_mebibytes: limit,
        ..options.clone()
    };
    assert!(generate_v1(&bounded, &root.join("bounded"))
        .unwrap_err()
        .contains("over max_batch_mebibytes"));
    let _ = fs::remove_dir_all(&root);
}

fn assert_verified_command_v1(
    root: &Path,
    fixture: &Path,
    manifest: &FixtureManifestV1,
    batch: &FixtureBatchV1,
) {
    let out = root.join("verified-command");
    let arguments = args(&[
        "command",
        &format!("fixture={}", fixture.display()),
        &format!("batch={}", batch.name),
        "path=serial",
        &format!("out={}", out.display()),
    ]);
    let default = main_v1(&arguments).unwrap();
    assert_eq!(default["mode"], "update");
    assert!(default.get("tape_cap_bytes").is_none());
    let cap = fixture_verified_cap_v1(batch);
    let mut arguments = arguments;
    arguments.push(format!("verified_replay_tape_bytes={cap}"));
    let command = main_v1(&arguments).unwrap();
    assert_eq!(command["mode"], "update_s_verified_serial");
    assert_eq!(command["tape_cap_bytes"], cap);
    let mut expected = default;
    expected["mode"] = command["mode"].clone();
    expected["tape_cap_bytes"] = json!(cap);
    assert_eq!(command, expected);

    for (field, value) in [
        ("preparation_workers", json!(1)),
        ("workers", json!(2)),
        ("ex1", json!({"w5":{}})),
        ("training_math", json!("portable-math-v1")),
    ] {
        let mut incompatible = command.clone();
        incompatible[field] = value;
        let error = serde_json::from_value::<ExpandedTrainingCommandV1>(incompatible).unwrap_err();
        assert!(error.to_string().contains("unknown field"), "{error}");
    }
    let mut missing_cap = command.clone();
    missing_cap.as_object_mut().unwrap().remove("tape_cap_bytes");
    let error = serde_json::from_value::<ExpandedTrainingCommandV1>(missing_cap).unwrap_err();
    assert!(error.to_string().contains("missing field `tape_cap_bytes`"));
    for (field, value, refusal) in [
        (
            "tape_cap_bytes",
            json!(0),
            "verified replay tape cap must be in",
        ),
        (
            "tape_cap_bytes",
            json!(67_108_865),
            "verified replay tape cap must be in",
        ),
        ("source", json!(manifest.net8), "requires an S learner source"),
        (
            "loss_selection",
            json!(ExpandedLossSelectionV1::TerminalReinforceValueV3),
            "an S learner trains only the GAE loss",
        ),
    ] {
        let mut incompatible = command.clone();
        incompatible[field] = value;
        let error = execute_v1(serde_json::from_value(incompatible).unwrap()).unwrap_err();
        assert!(error.contains(refusal), "{field}: {error}");
        assert!(!out.exists());
    }

    let result = execute_v1(serde_json::from_value(command).unwrap()).unwrap();
    assert_eq!(result["verified_replay"]["used"], true);
    assert_eq!(verify_v1(manifest, batch, &result).unwrap()["verified"], true);
}

#[test]
fn forward_attribution_requires_both_census_and_output_agreement() {
    use s_update_profile::{StageTotalsV1, StageV1};
    let census = BatchCensusV1 {
        learner_rows: 2,
        other_rows: 1,
        ..BatchCensusV1::default()
    };
    let mut totals = [StageTotalsV1::default(); s_update_profile::STAGES_V1];
    totals[StageV1::ReplayLearnerRow as usize].calls = 2;
    totals[StageV1::ReplayLearnerRow as usize].output_digest = 9;
    totals[StageV1::ReplayOtherRow as usize].calls = 1;
    totals[StageV1::LearnerForward as usize].calls = 2;
    totals[StageV1::LearnerForward as usize].output_digest = 10;
    totals[StageV1::LearnerBackward as usize].calls = 2;
    let mismatched = forward_profile_v1(Some(totals), &census, None);
    assert_eq!(mismatched["counts_match_census"], true);
    assert_eq!(mismatched["learner_forwards_repeat_replayed_rows"], false);
    assert!(mismatched["duplicate_forwards"].is_null());
    assert!(mismatched["duplicate_share_of_forward_calls"].is_null());

    totals[StageV1::LearnerForward as usize].output_digest = 9;
    let matched = forward_profile_v1(Some(totals), &census, None);
    assert_eq!(matched["duplicate_forwards"], 2);

    totals[StageV1::ReplayOtherRow as usize].calls = 0;
    let incomplete = forward_profile_v1(Some(totals), &census, None);
    assert_eq!(incomplete["counts_match_census"], false);
    assert!(incomplete["duplicate_forwards"].is_null());
}

#[test]
fn s_update_fixture_cli_refuses_unknown_repeated_and_out_of_range_arguments() {
    let root = temp_root("cli");
    let out = format!("out={}", root.join("never").display());
    let out = out.as_str();
    let seed = format!("seed={}", u64::MAX);
    for (arguments, refusal) in [
        (vec!["generate"], "out= is required"),
        (vec!["generate", out, "bogus=1"], "unknown argument"),
        (vec!["generate", out, "fixture=x"], "unknown argument"),
        (vec!["generate", out, out], "is repeated"),
        (vec!["generate", out, "episodes"], "expected key=value"),
        (vec!["generate", out, "episodes=0"], "episodes must be in"),
        (vec!["generate", out, "episodes=65"], "episodes must be in"),
        (vec!["generate", out, "episodes=two"], "not a valid number"),
        (
            vec!["generate", out, "shape=wide"],
            "shape must be test or s",
        ),
        (
            vec!["generate", out, "collection_workers=0"],
            "collection_workers",
        ),
        (
            vec!["generate", out, "max_batch_mebibytes=513"],
            "max_batch_mebibytes",
        ),
        (
            vec!["generate", out, "max_prepared_tensor_mebibytes=63"],
            "max_prepared_tensor_mebibytes",
        ),
        (vec!["generate", out, seed.as_str()], "seed leaves no room"),
        (
            vec!["bench", "fixture=x", "batch=initial", "path=fast", "out=y"],
            "path must be serial or prepared",
        ),
        (
            vec![
                "bench",
                "fixture=x",
                "batch=initial",
                "path=prepared",
                "verified_replay_tape_bytes=1048576",
                "out=y",
            ],
            "verified_replay_tape_bytes requires path=serial and workers=1",
        ),
        (
            vec![
                "command",
                "fixture=x",
                "batch=initial",
                "path=serial",
                "workers=2",
                "verified_replay_tape_bytes=1048576",
                "out=y",
            ],
            "verified_replay_tape_bytes requires path=serial and workers=1",
        ),
        (
            vec![
                "bench",
                "fixture=x",
                "batch=initial",
                "path=serial",
                "verified_replay_tape_bytes=0",
                "out=y",
            ],
            "verified replay tape cap must be in",
        ),
        (
            vec![
                "command",
                "fixture=x",
                "batch=initial",
                "path=serial",
                "verified_replay_tape_bytes=67108865",
                "out=y",
            ],
            "verified replay tape cap must be in",
        ),
        (
            vec![
                "bench",
                "fixture=x",
                "batch=initial",
                "path=serial",
                "input=z",
            ],
            "unknown argument",
        ),
        (
            vec![
                "command",
                "fixture=x",
                "batch=initial",
                "path=serial",
                "reps=2",
            ],
            "unknown argument",
        ),
        (
            vec!["verify", "fixture=x", "batch=initial"],
            "result= is required",
        ),
        (vec!["gae-bench"], "usage"),
        (vec![], "usage"),
    ] {
        let error = main_v1(&args(&arguments)).unwrap_err();
        assert!(error.contains(refusal), "{arguments:?}: {error}");
    }
    assert!(!root.join("never").exists());
    // A directory without a manifest is no fixture.
    let error = load_manifest_v1(&root).unwrap_err();
    assert!(error.contains(MANIFEST_FILE_V1), "{error}");
    let _ = fs::remove_dir_all(&root);
}
