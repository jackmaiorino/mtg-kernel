//! The saved-batch fixture: retained initial and warmed batches, exact
//! benches on every path, commands for older builds, verification,
//! root independence, and strict arguments.

use super::*;

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
        }
    }
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
    let mismatched = forward_profile_v1(Some(totals), &census);
    assert_eq!(mismatched["counts_match_census"], true);
    assert_eq!(mismatched["learner_forwards_repeat_replayed_rows"], false);
    assert!(mismatched["duplicate_forwards"].is_null());
    assert!(mismatched["duplicate_share_of_forward_calls"].is_null());

    totals[StageV1::LearnerForward as usize].output_digest = 9;
    let matched = forward_profile_v1(Some(totals), &census);
    assert_eq!(matched["duplicate_forwards"], 2);

    totals[StageV1::ReplayOtherRow as usize].calls = 0;
    let incomplete = forward_profile_v1(Some(totals), &census);
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
