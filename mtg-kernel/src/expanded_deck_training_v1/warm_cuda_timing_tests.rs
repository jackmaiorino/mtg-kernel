//! Warm/steady-state CUDA-vs-CPU timing diagnostic for the existing V3
//! update backend, over a pinned real trajectory batch (optionally tiled to
//! a larger substep count). Test-only, gated behind the CUDA feature. This
//! measures wall-clock only: no numerical acceptance envelope, no
//! playing-strength claim, and no production code path is changed. Every
//! call starts from a fresh clone of the same pinned initial state, so the
//! "warm" calls differ from the first ("cold") call only in whatever the
//! backend keeps resident (CUDA context, compiled kernels, allocator
//! arenas) between calls in the same process; they are not a second,
//! independent update chained onto the first.
//!
//! Batch provenance note: the originally intended fixture (the pinned
//! ten-episode, 654-substep real trajectory batch used by the earlier
//! windows-gpu-check-001 evidence) cannot be loaded at this commit.
//! `initialize()` fails closed with "destination card database differs
//! from the intended transfer": that fixture's model-parameter import was
//! stamped with an older merged card catalog hash
//! (`de59c501e943f3fd`), which no longer equals this commit's
//! `KERNEL_CARDDB_HASH` (`064a7c989255ab3c`, the Pauper-meta-wave-1
//! catalog; see `mtg-kernel/src/card_def.rs:1691`). This is a real,
//! pre-existing provenance gate, not specific to this test: the original
//! `cuda_probe_tests` probe over the same update command would fail the
//! same way at this commit. Producing a fresh pinned import compatible
//! with the current catalog is its own separate task (fresh
//! initialization plus a new collection pass), out of scope for a warm
//! timing measurement, so this diagnostic instead builds its batch from
//! the crate's own canonical, already-exercised in-process fixture
//! (`FrozenPlayPolicyV1::training_fixture_v3()` and the `tests` module's
//! `replay_fixture` helper, the same real game-engine forward path
//! `population_routes_distinct_parameters_and_physical_seat_rng_for_both_learner_seats`
//! already exercises up to 32 actions). Every substep is a genuine forward
//! pass through the real production model architecture and real encoded
//! game state; only the starting weights and the specific game line are
//! the crate's deterministic test fixture rather than a pinned production
//! checkpoint, which does not change the backend's per-substep wall clock
//! cost. The batch is then tiled to reach a realistic substep count, the
//! same tiling device used for the "large batch" scale-up.

#![cfg(feature = "experimental-burn-net8-packed-cuda-v1")]

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WarmTimingPlanV1 {
    device_ordinal: usize,
    /// Number of times the real, in-process-replayed batch is concatenated
    /// with itself before timing. This is a throughput scaling device, not
    /// additional independent evidence: a repeat greater than 1 must be
    /// reported as tiled, not as more games.
    batch_repeat: usize,
    /// Warm calls after the one cold call. Must be at least 10.
    warm_calls: usize,
    output_path: PathBuf,
}

fn gpu_memory_used_mib_v1(device_ordinal: usize) -> Result<u64, String> {
    let output = std::process::Command::new("C:/Windows/System32/nvidia-smi.exe")
        .args([
            "--query-gpu=memory.used",
            "--format=csv,noheader,nounits",
            "-i",
            &device_ordinal.to_string(),
        ])
        .output()
        .map_err(err)?;
    ensure(output.status.success(), "nvidia-smi memory query failed")?;
    let text = String::from_utf8(output.stdout).map_err(err)?;
    text.trim().parse::<u64>().map_err(err)
}

fn median_ms_v1(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite timing"));
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// One cold call followed by `warm_calls` warm calls of `backend`, each
/// starting from a fresh clone of `initial`. `backend` returns the elapsed
/// milliseconds for exactly the timed update call (nothing else).
fn run_calls_v1(
    initial: &NativePolicyValueTrainStateV1,
    warm_calls: usize,
    mut backend: impl FnMut(&mut NativePolicyValueTrainStateV1) -> Result<f64, String>,
) -> Result<Vec<Value>, String> {
    let mut calls = Vec::with_capacity(warm_calls + 1);
    for call_index in 0..=warm_calls {
        let mut state = initial.clone();
        let elapsed_ms = backend(&mut state)?;
        let state_sha256 = hex(&state.state_sha256_v1().map_err(err)?);
        let adam_step = state.snapshot_v1().map_err(err)?.adam_step;
        calls.push(json!({
            "call_index": call_index,
            "warm": call_index > 0,
            "elapsed_ms": elapsed_ms,
            "state_sha256": state_sha256,
            "adam_step": adam_step,
        }));
    }
    Ok(calls)
}

fn summarize_calls_v1(calls: &[Value]) -> Value {
    let warm: Vec<&Value> = calls.iter().skip(1).collect();
    let warm_ms: Vec<f64> = warm
        .iter()
        .map(|c| c["elapsed_ms"].as_f64().expect("elapsed_ms is f64"))
        .collect();
    let warm_hashes: Vec<&str> = warm
        .iter()
        .map(|c| c["state_sha256"].as_str().expect("state_sha256 is str"))
        .collect();
    let all_identical = warm_hashes.windows(2).all(|pair| pair[0] == pair[1]);
    json!({
        "cold_ms": calls[0]["elapsed_ms"],
        "cold_state_sha256": calls[0]["state_sha256"],
        "warm_ms": warm_ms,
        "warm_median_ms": median_ms_v1(&warm_ms),
        "warm_min_ms": warm_ms.iter().copied().fold(f64::INFINITY, f64::min),
        "warm_max_ms": warm_ms.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        "warm_state_hashes": warm_hashes,
        "warm_all_identical": all_identical,
        "warm_equals_cold": warm_hashes.first().is_some_and(|h| *h == calls[0]["state_sha256"]),
    })
}

fn run_warm_timing_v1(plan: WarmTimingPlanV1) -> Result<Value, String> {
    ensure(
        (1..=512).contains(&plan.batch_repeat),
        "invalid batch repeat",
    )?;
    ensure(plan.warm_calls >= 10, "at least ten warm calls required")?;
    ExpandedUpdateBackendV1::Cuda {
        device_ordinal: plan.device_ordinal,
    }
    .validate_v1()?;
    ExpandedUpdateBackendV1::Cuda {
        device_ordinal: plan.device_ordinal,
    }
    .require_compiled_v1()?;

    let learning_rate: f32 = 0.000_01;
    let value_coefficient: f32 = 0.5;

    // Same canonical fixture model the crate's own tests use (see module
    // doc comment above for why this replaces the pinned real fixture).
    let mut learner_policy = FrozenPlayPolicyV1::training_fixture_v3();
    let mut model = NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
        .map_err(err)?;
    model
        .replace_parameter_snapshot_v1(&learner_policy.training_parameters_v3())
        .map_err(err)?;
    let initial = NativePolicyValueTrainStateV1::new_v1(model).map_err(err)?;
    let before = hex(&initial.state_sha256_v1().map_err(err)?);

    // 32 real, actor-visible decisions (self-play: no distinct opponent),
    // the same actor pattern already proven at this length by
    // `population_routes_distinct_parameters_and_physical_seat_rng_for_both_learner_seats`.
    let actors: Vec<u8> = [0_u8, 1, 1, 0, 1, 0, 0, 1].repeat(4);
    let (episode, learner_behavior, opponent_behavior) =
        super::tests::replay_fixture(&mut learner_policy, None, 0, &actors);
    ensure(opponent_behavior.is_none(), "unexpected synthetic opponent")?;
    validate_trajectory(&episode)?;
    validate_actual_behaviors_v1(&episode, &learner_behavior, None)?;
    let episodes_len = 1_usize;
    let tensor_groups = replay_learner_groups_v1(&episode, &learner_policy, None)?;
    ensure(!tensor_groups.is_empty(), "no learner probe groups")?;
    let substeps: Vec<Vec<_>> = tensor_groups
        .iter()
        .map(|(_, rows)| {
            rows.iter()
                .map(|(row, t)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        encoded_decision_view_generic_v1(t, FreshLineageGenerationV1::V3),
                    )),
                    selected_action_index: row.selected as usize,
                    expected_raw_action_logit_bits: &row.logits,
                    expected_value_bits: row.value,
                })
                .collect()
        })
        .collect();
    let real_substep_count: usize = substeps.iter().map(Vec::len).sum();
    let base_groups: Vec<_> = substeps
        .iter()
        .zip(&tensor_groups)
        .map(|(steps, (reward, _))| NativePolicyPhysicalDecisionV1 {
            substeps: steps,
            terminal_return: *reward,
            baseline_bits: 0,
        })
        .collect();
    // Tiling: the same real, pinned groups repeated `batch_repeat` times.
    // `NativePolicyPhysicalDecisionV1` is `Copy` (a slice reference plus two
    // small scalars), so this aliases the already-loaded substep rows rather
    // than duplicating trajectory data on disk or re-parsing anything.
    let groups: Vec<_> = std::iter::repeat(base_groups.as_slice())
        .take(plan.batch_repeat)
        .flatten()
        .copied()
        .collect();

    initial
        .validate_cuda_feature_transfer_update_v3(
            &groups,
            value_coefficient,
            learning_rate,
            plan.device_ordinal,
        )
        .map_err(err)?;

    let gpu_memory_before_any_call = gpu_memory_used_mib_v1(plan.device_ordinal)?;

    let cpu_calls = run_calls_v1(&initial, plan.warm_calls, |state| {
        let started = std::time::Instant::now();
        let result = state
            .train_step_feature_transfer_v3(&groups, value_coefficient, learning_rate)
            .map_err(err)?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        ensure(result.gradients.len() == 33, "CPU call needs all 33 gradients")?;
        Ok(elapsed_ms)
    })?;

    let mut gpu_memory_per_cuda_call = Vec::with_capacity(plan.warm_calls + 1);
    let cuda_calls = run_calls_v1(&initial, plan.warm_calls, |state| {
        let started = std::time::Instant::now();
        let result = state
            .train_step_cuda_feature_transfer_v3(
                &groups,
                value_coefficient,
                learning_rate,
                plan.device_ordinal,
            )
            .map_err(err)?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        // The plain production CUDA entry point (unlike the test-only
        // `_capture_v3`/`_capture_numerics_v3` siblings) always returns
        // `gradients: Vec::new()`: named per-parameter gradient export is
        // gated behind a `capture_named_gradients` flag this call path
        // passes as `false`, matching what `execute_update_v1` actually
        // runs in production. Sanity-check the real result fields instead.
        ensure(result.gradients.is_empty(), "unexpected CUDA named gradients")?;
        ensure(result.loss.is_finite(), "CUDA call produced a non-finite loss")?;
        gpu_memory_per_cuda_call.push(gpu_memory_used_mib_v1(plan.device_ordinal)?);
        Ok(elapsed_ms)
    })?;

    let gpu_memory_after_all_calls = gpu_memory_used_mib_v1(plan.device_ordinal)?;

    let cpu_summary = summarize_calls_v1(&cpu_calls);
    let cuda_summary = summarize_calls_v1(&cuda_calls);
    let cpu_warm_median = cpu_summary["warm_median_ms"].as_f64().unwrap_or(f64::NAN);
    let cuda_warm_median = cuda_summary["warm_median_ms"].as_f64().unwrap_or(f64::NAN);
    let cpu_cold = cpu_summary["cold_ms"].as_f64().unwrap_or(f64::NAN);
    let cuda_cold = cuda_summary["cold_ms"].as_f64().unwrap_or(f64::NAN);

    let result = json!({
        "schema": "mtg-kernel-v3-warm-cuda-timing/v1",
        "claim": "Warm/steady-state in-process CUDA-vs-CPU timing diagnostic on the existing V3 update backend only; no numerical acceptance envelope, no V4 arm, no playing-strength or speed qualification.",
        "batch_provenance": "Crate's own canonical in-process fixture (FrozenPlayPolicyV1::training_fixture_v3+ the tests module's replay_fixture over 32 real actor decisions, self-play), tiled batch_repeat times. NOT the pinned windows-gpu-check-001 real-trajectory fixture: that fixture's model-parameter import is stamped with an older card catalog hash (de59c501e943f3fd) that no longer equals this commit's KERNEL_CARDDB_HASH (064a7c989255ab3c); initialize() fails closed with 'destination card database differs from the intended transfer'. Every substep here is still a genuine forward/backward pass through the real production model architecture and real encoded game state.",
        "device_ordinal": plan.device_ordinal,
        "batch_repeat": plan.batch_repeat,
        "batch_is_tiled": plan.batch_repeat > 1,
        "real_episodes": episodes_len,
        "real_physical_groups": base_groups.len(),
        "real_substeps": real_substep_count,
        "physical_groups": groups.len(),
        "substeps": real_substep_count * plan.batch_repeat,
        "initial_state_sha256": before,
        "learning_rate": learning_rate,
        "value_coefficient": value_coefficient,
        "warm_calls": plan.warm_calls,
        "gpu_memory_mib": {
            "device_ordinal": plan.device_ordinal,
            "before_any_call": gpu_memory_before_any_call,
            "after_all_calls": gpu_memory_after_all_calls,
            "per_cuda_call": gpu_memory_per_cuda_call,
            "max_observed": gpu_memory_per_cuda_call.iter().copied().max(),
            "min_observed": gpu_memory_per_cuda_call.iter().copied().min(),
        },
        "cpu": {"backend": "train_step_feature_transfer_v3", "calls": cpu_calls, "summary": cpu_summary},
        "cuda": {"backend": "train_step_cuda_feature_transfer_v3", "calls": cuda_calls, "summary": cuda_summary},
        "ratio_warm_median_cuda_over_cpu": cuda_warm_median / cpu_warm_median,
        "ratio_cold_cuda_over_cpu": cuda_cold / cpu_cold,
    });
    if let Some(parent) = plan.output_path.parent() {
        fs::create_dir_all(parent).map_err(err)?;
    }
    fs::write(
        &plan.output_path,
        serde_json::to_vec_pretty(&result).map_err(err)?,
    )
    .map_err(err)?;
    Ok(result)
}

fn explicit_plan_v1() -> WarmTimingPlanV1 {
    let path = std::env::var("MTG_KERNEL_V3_WARM_TIMING_PLAN")
        .expect("explicit warm timing plan required");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "requires separately scheduled explicit GPU execution; warm/steady-state timing diagnostic only"]
fn v3_warm_steady_state_cuda_vs_cpu_timing() {
    let result = run_warm_timing_v1(explicit_plan_v1()).unwrap();
    println!("{result}");
}
