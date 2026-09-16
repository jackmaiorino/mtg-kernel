//! Explicit, ignored diagnostics over pinned real V3 trajectories.
//! CPU preparation is separate from device execution. Neither declares a
//! numerical acceptance envelope or supplies playing-strength evidence.

use super::*;
use crate::native_policy_train_step_v1::NativePolicyTrainStepResultV1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbePlanV1 {
    update_command: PinnedFileV1,
    output_directory: PathBuf,
    device_ordinal: usize,
}

fn named_bits(values: &[NativeNamedParameterV1]) -> Vec<ParameterBitsV1> {
    values.iter().map(ParameterBitsV1::from_native).collect()
}

fn snapshot_json(s: &NativePolicyValueTrainSnapshotV1) -> Value {
    json!({
        "schema": "mtg-kernel-v3-numerical-snapshot/v1",
        "float_encoding": "ieee754-binary32-u32-bits",
        "state_sha256": hex(&s.state_sha256_v1().unwrap()),
        "adam_step": s.adam_step,
        "scorer_bias_anchor_bits": s.scorer_bias_anchor_bits,
        "parameters": named_bits(&s.parameters),
        "first_moments": named_bits(&s.first_moments),
        "second_moments": named_bits(&s.second_moments),
    })
}

fn result_json(r: &NativePolicyTrainStepResultV1, loss_source: &str) -> Value {
    json!({
        "float_encoding": "ieee754-binary32-u32-bits",
        "loss_source": loss_source,
        "loss_bits": r.loss.to_bits(), "policy_sum_bits": r.policy_sum.to_bits(),
        "value_sum_bits": r.value_sum.to_bits(), "adam_step": r.adam_step,
        "gradients": named_bits(&r.gradients),
        "gauge_raw_residual_bits": r.scorer_bias_gauge.raw_gradient_residual.to_bits(),
        "gauge_derived_bound": r.scorer_bias_gauge.derived_absolute_bound,
    })
}

fn run_probe_v1(plan: ProbePlanV1, with_device: bool) -> Result<Value, String> {
    ensure(
        !plan.output_directory.exists(),
        "fresh probe directory required",
    )?;
    ExpandedUpdateBackendV1::Cuda {
        device_ordinal: plan.device_ordinal,
    }
    .validate_v1()?;
    if with_device {
        ExpandedUpdateBackendV1::Cuda {
            device_ordinal: plan.device_ordinal,
        }
        .require_compiled_v1()?;
    }
    let command: ExpandedTrainingCommandV1 = read_pinned(&plan.update_command)?;
    let ExpandedTrainingCommandV1::Update {
        source,
        trajectories,
        learning_rate,
        value_coefficient,
        ..
    } = command
    else {
        return Err("probe requires an update command".into());
    };
    ensure(
        !trajectories.is_empty() && trajectories.len() <= 1024,
        "invalid probe batch",
    )?;
    let mut total_bytes = 0_u64;
    for pin in &trajectories {
        total_bytes = total_bytes
            .checked_add(fs::metadata(&pin.path).map_err(err)?.len())
            .ok_or("probe batch size overflow")?;
        ensure(
            total_bytes <= MAX_BATCH_BYTES,
            "probe batch exceeds ingestion bound",
        )?;
    }
    let (policy, initial) = initialize(&source)?;
    let initial_snapshot = initial.snapshot_v1().map_err(err)?;
    let before = hex(&initial.state_sha256_v1().map_err(err)?);
    let learner = ExpandedSeatBehaviorV1 {
        source: source.clone(),
        identity: inference_identity_v1(&source, &policy, &initial)?,
    };
    let mut ids = BTreeSet::new();
    let mut hashes = BTreeSet::new();
    let mut episodes = Vec::new();
    for pin in &trajectories {
        ensure(
            hashes.insert(&pin.sha256),
            "duplicate probe trajectory bytes",
        )?;
        let episode: ExpandedTrajectoryV1 = read_pinned(pin)?;
        validate_trajectory(&episode)?;
        ensure(
            ids.insert(episode.episode.id.clone()),
            "duplicate probe episode",
        )?;
        ensure(
            episode.behavior_state_sha256 == before,
            "stale probe trajectory",
        )?;
        ensure(
            episode.source_import == *policy.identity_v1(),
            "probe import differs",
        )?;
        episodes.push(episode);
    }
    let mut cache = OpponentCacheV1::default();
    let mut tensor_groups = Vec::new();
    let mut group_episode_ids = Vec::new();
    for episode in &episodes {
        let opponent = episode
            .episode
            .opponent
            .as_ref()
            .map(|s| cache.load(s))
            .transpose()?;
        validate_actual_behaviors_v1(episode, &learner, opponent.as_ref().map(|o| &o.behavior))?;
        let episode_groups =
            replay_learner_groups_v1(episode, &policy, opponent.as_ref().map(|o| &o.policy))?;
        group_episode_ids.extend(std::iter::repeat_n(
            episode.episode.id.clone(),
            episode_groups.len(),
        ));
        tensor_groups.extend(episode_groups);
    }
    drop(cache);
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
    let groups: Vec<_> = substeps
        .iter()
        .zip(&tensor_groups)
        .map(|(steps, (reward, _))| NativePolicyPhysicalDecisionV1 {
            substeps: steps,
            terminal_return: *reward,
            baseline_bits: 0,
        })
        .collect();
    initial
        .validate_cuda_feature_transfer_update_v3(
            &groups,
            value_coefficient,
            learning_rate,
            plan.device_ordinal,
        )
        .map_err(err)?;

    let mut cpu = initial.clone();
    let cpu_started = std::time::Instant::now();
    let cpu_result = cpu
        .train_step_feature_transfer_v3(&groups, value_coefficient, learning_rate)
        .map_err(err)?;
    let cpu_seconds = cpu_started.elapsed().as_secs_f64();
    ensure(
        cpu_result.gradients.len() == 33,
        "CPU probe needs all 33 gradients",
    )?;
    let cpu_snapshot = cpu.snapshot_v1().map_err(err)?;
    ensure(
        cpu_snapshot.adam_step == initial_snapshot.adam_step + 1,
        "CPU probe step differs",
    )?;
    fs::create_dir(&plan.output_directory).map_err(err)?;
    let inputs = json!({
        "schema": "mtg-kernel-v3-numerical-probe-input/v1",
        "update_command": plan.update_command, "source": source,
        "trajectory_pins": trajectories, "learner_identity": learner.identity,
        "learning_rate_bits": learning_rate.to_bits(), "value_coefficient_bits": value_coefficient.to_bits(),
        "feature_contract_digest": FEATURE_CONTRACT_DIGEST_V3,
        "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V3,
        "group_episode_ids": group_episode_ids,
        "groups": tensor_groups.iter().map(|(reward, rows)| json!({
            "terminal_return": reward, "baseline_bits": 0,
            "rows": rows.iter().map(|(row, _)| *row).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
        "episode_ids": episodes.iter().map(|e| &e.episode.id).collect::<Vec<_>>(),
    });
    let mut outputs = vec![
        publish_json(&plan.output_directory, "inputs.json", &inputs)?,
        publish_json(
            &plan.output_directory,
            "initial.json",
            &snapshot_json(&initial_snapshot),
        )?,
        publish_json(
            &plan.output_directory,
            "cpu-after.json",
            &snapshot_json(&cpu_snapshot),
        )?,
        publish_json(
            &plan.output_directory,
            "cpu-result.json",
            &result_json(&cpu_result, "actual-cpu-forward"),
        )?,
    ];
    let mut cuda_seconds: Option<f64> = None;
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    if with_device {
        let mut gpu = initial.clone();
        let started = std::time::Instant::now();
        let captured = crate::experimental_burn_net8_packed_v1::bridge::train_step_cuda_burn_dense_feature_transfer_capture_numerics_v3(
            &mut gpu, &groups, value_coefficient, learning_rate, plan.device_ordinal,
        ).map_err(err)?;
        cuda_seconds = Some(started.elapsed().as_secs_f64());
        ensure(
            captured.result.gradients.len() == 33,
            "CUDA probe needs all 33 gradients",
        )?;
        let snapshot = gpu.snapshot_v1().map_err(err)?;
        ensure(
            snapshot.adam_step == initial_snapshot.adam_step + 1,
            "CUDA probe step differs",
        )?;
        outputs.push(publish_json(
            &plan.output_directory,
            "cuda-after.json",
            &snapshot_json(&snapshot),
        )?);
        outputs.push(publish_json(
            &plan.output_directory,
            "cuda-result.json",
            &result_json(&captured.result, "transported-cpu-outputs"),
        )?);
        outputs.push(publish_json(
            &plan.output_directory,
            "cuda-device.json",
            &captured.device,
        )?);
    }
    #[cfg(not(feature = "experimental-burn-net8-packed-cuda-v1"))]
    let _ = (&mut outputs, &mut cuda_seconds);
    let result = json!({
        "schema": "mtg-kernel-v3-numerical-probe/v1", "complete": true,
        "device_work_performed": with_device,
        "gpu_ordinal": if with_device { Some(plan.device_ordinal) } else { None },
        "physical_groups": groups.len(), "substeps": substeps.iter().map(Vec::len).sum::<usize>(),
        "initial_state_sha256": before,
        "cpu_update_seconds": cpu_seconds, "cuda_capture_update_seconds": cuda_seconds,
        "outputs": outputs,
        "claim": "Raw diagnostic evidence only; no acceptance envelope, numerical qualification, speedup or strength claim",
    });
    publish_json(&plan.output_directory, "completion.json", &result)?;
    Ok(result)
}

fn explicit_plan_v1() -> ProbePlanV1 {
    let path =
        std::env::var("MTG_KERNEL_V3_NUMERICAL_PROBE_PLAN").expect("explicit probe plan required");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "explicit CPU-only reference preparation over pinned local trajectories"]
fn v3_real_trajectory_cpu_reference_export() {
    let result = run_probe_v1(explicit_plan_v1(), false).unwrap();
    assert_eq!(result["device_work_performed"], false);
    println!("{}", result);
}

#[test]
#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
#[ignore = "requires separately scheduled explicit GPU execution; raw diagnostic only"]
fn v3_real_trajectory_cuda_numerical_export() {
    let result = run_probe_v1(explicit_plan_v1(), true).unwrap();
    assert_eq!(result["device_work_performed"], true);
    println!("{}", result);
}
