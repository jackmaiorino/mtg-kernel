//! Saved initial-batch qualification only. No collector, actor-session step,
//! sampling-policy change, training campaign, or strength comparison lives here.
//! All backward/update calls use the existing public CUDA implementation.
use super::*;
use crate::experimental_burn_net8_packed_v1::public_training::PublicGroupGradientDiagnostic;

const SCHEMA: &str = "public-ppo-saved-group-qualification/v1";

/// Elementwise envelope: |candidate - reference| <= absolute +
/// relative * |reference|. Absolute units are the tensor's own scalar units;
/// relative is dimensionless. Gradients use GAE as the reference. The update
/// envelope applies separately to base parameters and both Adam moments.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub absolute: f64,
    pub relative: f64,
}

impl Envelope {
    fn validate(self) -> Result<(), String> {
        ensure(
            self.absolute.is_finite()
                && self.relative.is_finite()
                && self.absolute >= 0.0
                && self.relative >= 0.0
                && (self.absolute + self.relative * f64::from(f32::MAX)).is_finite()
                && (self.absolute > 0.0 || self.relative > 0.0),
            "qualification envelope must be finite, nonnegative and nonzero",
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelopes {
    pub gradient: Envelope,
    pub update: Envelope,
    /// Maximum absolute natural log of the initial per-physical-group ratio.
    pub max_initial_log_ratio_error: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub schema: String,
    /// Pinned JSON ExpandedModelSourceV1 descriptor, including its pinned parent.
    pub source: PinnedFileV1,
    /// Exact public training Config that collected the initial batch.
    pub collection_config: PinnedFileV1,
    /// Complete initial batch, in config.updates[0] schedule order.
    pub trajectories: Vec<PinnedFileV1>,
    pub expected_initial_adam_step: u64,
    pub execution_gpu_ordinal: usize,
    /// Launcher compatibility only. This binary collects no games.
    pub collector_workers: usize,
    pub envelopes: Envelopes,
    pub output_directory: PathBuf,
}

fn validate_command(command: &Command) -> Result<(), String> {
    ensure(command.schema == SCHEMA, "qualification schema differs")?;
    ensure(
        command.execution_gpu_ordinal == 1
            && command.collector_workers == 1
            && command.expected_initial_adam_step > 0
            && (1..=64).contains(&command.trajectories.len()),
        "qualification requires GPU 1, one launcher worker and a bounded nonempty batch",
    )?;
    command.envelopes.gradient.validate()?;
    command.envelopes.update.validate()?;
    ensure(
        command.envelopes.max_initial_log_ratio_error.is_finite()
            && command.envelopes.max_initial_log_ratio_error >= 0.0,
        "initial log-ratio envelope must be finite and nonnegative",
    )
}

#[derive(Serialize)]
struct Comparison {
    scalars: usize,
    outside_envelope: usize,
    max_absolute_error: f64,
    max_error_over_bound: f64,
    worst_tensor: String,
    worst_index: usize,
}

impl Comparison {
    fn new() -> Self {
        Self {
            scalars: 0,
            outside_envelope: 0,
            max_absolute_error: 0.0,
            max_error_over_bound: 0.0,
            worst_tensor: String::new(),
            worst_index: 0,
        }
    }

    fn observe(
        &mut self,
        name: &str,
        index: usize,
        reference: f32,
        candidate: f32,
        envelope: Envelope,
    ) -> Result<(), String> {
        ensure(
            reference.is_finite() && candidate.is_finite(),
            &format!("nonfinite diagnostic scalar {name}[{index}]"),
        )?;
        let error = (f64::from(candidate) - f64::from(reference)).abs();
        let bound = envelope.absolute + envelope.relative * f64::from(reference).abs();
        let scaled = if bound > 0.0 {
            (error / bound).min(f64::MAX)
        } else if error == 0.0 {
            0.0
        } else {
            f64::MAX
        };
        self.scalars += 1;
        self.outside_envelope += usize::from(error > bound);
        self.max_absolute_error = self.max_absolute_error.max(error);
        if scaled > self.max_error_over_bound {
            self.max_error_over_bound = scaled;
            self.worst_tensor = name.into();
            self.worst_index = index;
        }
        Ok(())
    }

    fn named(
        &mut self,
        prefix: &str,
        reference: &[NativeNamedParameterV1],
        candidate: &[NativeNamedParameterV1],
        envelope: Envelope,
    ) -> Result<(), String> {
        ensure(
            reference.len() == candidate.len(),
            "comparison tensor count differs",
        )?;
        for (a, b) in reference.iter().zip(candidate) {
            ensure(
                a.name == b.name && a.shape == b.shape && a.values.len() == b.values.len(),
                "comparison tensor layout differs",
            )?;
            let name = format!("{prefix}/{}", a.name);
            for (i, (&a, &b)) in a.values.iter().zip(&b.values).enumerate() {
                self.observe(&name, i, a, b, envelope)?;
            }
        }
        Ok(())
    }
}

fn gradient_document(gradient: &PublicGroupGradientDiagnostic) -> Value {
    let base: Vec<_> = gradient
        .base
        .iter()
        .map(|row| json!({"name":row.name,"shape":row.shape,"values_bits":bits(&row.values)}))
        .collect();
    json!({"schema":"public-group-effective-gradient/v1","base":base,
        "public_object_bits":gradient.object_bits,"public_state_bits":gradient.state_bits,
        "ratios":gradient.ratios,"scorer_gauge":"zeroed exactly as production Adam apply"})
}

fn validate_gradient(gradient: &PublicGroupGradientDiagnostic) -> Result<(), String> {
    ensure(
        gradient
            .base
            .iter()
            .flat_map(|r| &r.values)
            .all(|v| v.is_finite())
            && gradient
                .object_bits
                .iter()
                .chain(&gradient.state_bits)
                .all(|b| f32::from_bits(*b) == 0.0),
        "nonfinite base gradient or nonzero disabled projection gradient",
    )
}

fn state_bytes(
    state: &mut PublicDeviceTrainState,
    config: &Config,
    base_age: u64,
    added_steps: u64,
) -> Result<(NativePolicyValueTrainSnapshotV1, Vec<u8>), String> {
    let (base, public) = state.snapshot().map_err(err)?;
    validate_projection_mode(config, &public)?;
    ensure(
        base.adam_step == base_age + added_steps && public.adam_step == added_steps,
        "qualification optimizer ages differ",
    )?;
    let bytes = public_training::snapshot::encode(&base, &public).map_err(err)?;
    Ok((base, bytes))
}

/// Validates and scores saved learner tensors. The draw check invokes the
/// production quantized sampler on both score arrays with the same recorded
/// learner-seat RNG stream. It does not reconstruct or step a game session.
struct PreparedCpu<'a> {
    records: Vec<LearnerPublicGroupV1<'a>>,
    targets: Vec<f32>,
    advantages: Vec<f32>,
    report: Value,
}

fn prepare_cpu<'a>(
    trajectories: &'a [Trajectory],
    base: &FrozenPlayPolicyV1,
    wrapper: &PublicInputPlayPolicyV1,
) -> Result<PreparedCpu<'a>, String> {
    let mut records = Vec::new();
    let mut raw_advantages = Vec::new();
    let mut targets = Vec::new();
    let mut telemetry = Vec::new();
    let mut sampler = WideCategoricalScratchV1::default();
    let mut choices = 0;
    for trajectory in trajectories {
        let groups = learner_groups(trajectory)?;
        ensure(!groups.is_empty(), "trajectory has no learner groups")?;
        let values: Vec<_> = groups
            .iter()
            .enumerate()
            .map(|(i, g)| (f32::from_bits(g[0].0.value), i + 1 == groups.len()))
            .collect();
        for (advantage, target) in gae_episode_advantages_v1(
            &values,
            trajectory.terminal.terminal_reward[trajectory.episode.learner_seat as usize] as f32,
            1.0,
            0.9,
        )? {
            raw_advantages.push(advantage);
            targets.push(target);
        }
        let mut rng = SplitMix64::seed(
            paired_policy_seeds_v1(trajectory.episode.seed)
                [trajectory.episode.learner_seat as usize],
        );
        for group in &groups {
            let mut log_q = 0.0_f64;
            let mut maximum_selected_log_p_over_q = 0.0_f64;
            for (row, tensor, auxiliary) in group {
                let a = base.score_training_tensor_v4(tensor)?;
                let b = wrapper.replay(tensor, auxiliary)?;
                ensure(
                    bits(&a.logits) == row.logits
                        && bits(&b.logits) == row.logits
                        && a.value.to_bits() == row.value
                        && b.value.to_bits() == row.value,
                    &format!(
                        "disabled wrapper/base/behavior CPU scores differ at {} step {}",
                        trajectory.episode.id, row.step
                    ),
                )?;
                let seed = rng.next_u64();
                let a_selected = sampler.sample(&a.logits, seed).map_err(err)?;
                let b_selected = sampler.sample(&b.logits, seed).map_err(err)?;
                ensure(
                    a_selected == b_selected && b_selected == row.selected as usize,
                    "disabled wrapper/base sampler draws differ from behavior",
                )?;
                let q = sampler.last_masses_v1(row.logits.len())[b_selected] as f64
                    / 18446744073709551616.0_f64;
                ensure(q > 0.0 && q.is_finite(), "invalid selected executable mass")?;
                let log_p = f64::from(behavior_joint_log_probability(&[(
                    &row.logits,
                    b_selected,
                )])?);
                ensure(
                    log_p.is_finite(),
                    "selected raw log probability is not finite",
                )?;
                log_q += q.ln();
                maximum_selected_log_p_over_q =
                    maximum_selected_log_p_over_q.max((log_p - q.ln()).abs());
                choices += usize::from(row.logits.len() > 1);
            }
            let behavior = behavior_joint_log_probability(
                &group
                    .iter()
                    .map(|(row, _, _)| (&row.logits[..], row.selected as usize))
                    .collect::<Vec<_>>(),
            )?;
            ensure(
                behavior.is_finite(),
                "raw joint log probability is not finite",
            )?;
            telemetry.push(json!({"group":telemetry.len(),
                "episode_id":trajectory.episode.id,"physical_decision_id":group[0].0.physical_decision_id,
                "substeps":group.len(),"behavior_raw_joint_log_probability":behavior,
                "executable_selected_joint_log_probability":log_q,
                "selected_joint_log_p_over_q":f64::from(behavior)-log_q,
                "max_substep_abs_selected_log_p_over_q":maximum_selected_log_p_over_q}));
        }
        records.extend(groups);
    }
    ensure(choices > 0, "saved batch has no non-forced learner choices")?;
    let (advantages, statistics) = normalize_advantages_v1(&raw_advantages, 1e-6)?;
    Ok(PreparedCpu {
        records,
        targets,
        advantages,
        report: json!({"groups":telemetry,"advantage_statistics":statistics,
        "nonforced_learner_substeps":choices,
        "draw_check":"production sampler on both replayed score arrays using the recorded learner-seat stream; no session stepping",
        "p_over_q_semantics":"selected-action likelihood discrepancy only, not a full-distribution divergence"}),
    })
}

pub fn run(command: Command) -> Result<Value, String> {
    validate_command(&command)?;
    let source: ExpandedModelSourceV1 =
        serde_json::from_slice(&read_pinned_bytes(&command.source)?).map_err(err)?;
    let config: Config =
        serde_json::from_slice(&read_pinned_bytes(&command.collection_config)?).map_err(err)?;
    validate_entropy(&config)?;
    ensure(
        source == config.source
            && source.checkpoint.is_some()
            && !config.inputs_enabled
            && config.learning_rate.to_bits() == 0.0001_f32.to_bits()
            && config.value_coefficient.to_bits() == 0.5_f32.to_bits()
            && config.gamma.to_bits() == 1.0_f32.to_bits()
            && config.lambda.to_bits() == 0.9_f32.to_bits()
            && config.entropy_coefficient == 0.0
            && config.gpu_ordinal == 1
            && (1..=512).contains(&config.max_chunk_substeps)
            && config
                .updates
                .first()
                .is_some_and(|u| u.len() == command.trajectories.len()),
        "qualification source/disabled-input/fixed-scalar/initial-batch contract differs",
    )?;
    let config_hash = sha(&serde_json::to_vec(&config).map_err(err)?);
    let (base, initial) = initialize(&source)?;
    ensure(
        base.feature_identity_v1().generation == FreshLineageGenerationV1::V4
            && initial.adam_step_v1() == command.expected_initial_adam_step,
        "qualification parent feature generation or Adam age differs",
    )?;
    let legacy = initial.snapshot_v1().map_err(err)?;
    let public = ProjectionSnapshot::zero();
    validate_projection_mode(&config, &public)?;
    let initial_bytes = public_training::snapshot::encode(&legacy, &public).map_err(err)?;
    let initial_hash = sha(&initial_bytes);
    let mut trajectory_bytes = 0_u64;
    let mut unique = BTreeSet::new();
    let mut trajectories = Vec::new();
    for (i, pin) in command.trajectories.iter().enumerate() {
        ensure(
            unique.insert(&pin.sha256),
            "duplicate qualification trajectory",
        )?;
        let bytes = read_pinned_bytes(pin)?;
        trajectory_bytes += bytes.len() as u64;
        ensure(
            trajectory_bytes <= MAX_BATCH_BYTES,
            "qualification batch exceeds 512 MiB",
        )?;
        let t: Trajectory = serde_json::from_slice(&bytes).map_err(err)?;
        ensure(
            replay_audit::admits_public_trajectory(&t)
                && !t.inputs_enabled
                && t.learner_sampler.is_none()
                && t.config_sha256 == config_hash
                && t.optimizer_state_sha256 == initial_hash
                && serde_json::to_vec(&t.episode).map_err(err)?
                    == serde_json::to_vec(&config.updates[0][i]).map_err(err)?,
            "qualification trajectory is not the complete initial current-parent public batch",
        )?;
        t.validate_records()?;
        trajectories.push(t);
    }
    fs::create_dir_all(&command.output_directory).map_err(err)?;
    ensure(
        fs::read_dir(&command.output_directory)
            .map_err(err)?
            .next()
            .is_none(),
        "qualification output directory is not empty",
    )?;
    publish_json(&command.output_directory, "request.json", &command)?;
    let wrapper = PublicInputPlayPolicyV1::new(base.fork_for_collection_v3()?, weights(&public)?)?
        .with_inputs_enabled(false);
    let PreparedCpu {
        records,
        targets,
        advantages,
        report: cpu,
    } = prepare_cpu(&trajectories, &base, &wrapper)?;
    ensure(
        records.len() <= 1_000_000,
        "qualification exceeds production group bound",
    )?;
    let steps: Vec<Vec<_>> = records
        .iter()
        .map(|group| {
            group
                .iter()
                .map(|(row, tensor, auxiliary)| PublicTrainingStep {
                    view: encoded_decision_view_v4(tensor),
                    auxiliary,
                    selected: row.selected as usize,
                    expected_logits: &row.logits,
                    expected_value: row.value,
                })
                .collect()
        })
        .collect();
    let groups: Vec<_> = steps
        .iter()
        .map(|substeps| PublicTrainingGroup { substeps })
        .collect();
    let behavior_joint: Vec<_> = steps
        .iter()
        .map(|group| {
            behavior_joint_log_probability(
                &group
                    .iter()
                    .map(|s| (s.expected_logits, s.selected))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Result<_, _>>()?;
    publish_json(
        &command.output_directory,
        "saved-groups.json",
        &json!({"cpu":cpu,
        "value_targets_bits":bits(&targets),"normalized_advantages_bits":bits(&advantages),
        "behavior_joint_bits":bits(&behavior_joint)}),
    )?;
    let device = burn_cuda::CudaDevice::new(command.execution_gpu_ordinal);
    let import = || PublicDeviceTrainState::import(&legacy, &public, &device).map_err(err);
    let mut gae = import()?;
    let (_, imported_bytes) = state_bytes(&mut gae, &config, legacy.adam_step, 0)?;
    ensure(
        imported_bytes == initial_bytes,
        "CUDA import altered complete parent optimizer state",
    )?;
    publish_bytes(
        &command.output_directory,
        "imported-optimizer.json",
        &imported_bytes,
    )?;
    let gae_gradient = gae
        .diagnostic_group_gradients(
            &groups,
            &targets,
            &advantages,
            None,
            config.max_chunk_substeps,
        )
        .map_err(err)?;
    validate_gradient(&gae_gradient)?;
    let mut ppo = import()?;
    let ppo_gradient = ppo
        .diagnostic_group_gradients(
            &groups,
            &targets,
            &advantages,
            Some((&behavior_joint, 0.2)),
            config.max_chunk_substeps,
        )
        .map_err(err)?;
    validate_gradient(&ppo_gradient)?;
    publish_json(
        &command.output_directory,
        "gae-gradients.json",
        &gradient_document(&gae_gradient),
    )?;
    publish_json(
        &command.output_directory,
        "ppo-gradients.json",
        &gradient_document(&ppo_gradient),
    )?;
    let mut gradient_comparison = Comparison::new();
    gradient_comparison.named(
        "gradient",
        &gae_gradient.base,
        &ppo_gradient.base,
        command.envelopes.gradient,
    )?;
    ensure(
        ppo_gradient.ratios.len() == groups.len(),
        "initial ratio count differs",
    )?;
    let mut ratios = Vec::new();
    let mut max_log_ratio_error = 0.0_f64;
    for (g, &ratio) in ppo_gradient.ratios.iter().enumerate() {
        ensure(
            ratio.is_finite() && ratio > 0.0,
            "initial PPO ratio is invalid",
        )?;
        let error = f64::from(ratio).ln().abs();
        max_log_ratio_error = max_log_ratio_error.max(error);
        ratios
            .push(json!({"group":g,"substeps":steps[g].len(),"ratio":ratio,"abs_log_ratio":error}));
    }
    gae.update_groups(
        &groups,
        &targets,
        &advantages,
        0.0001,
        0.5,
        0.0,
        false,
        true,
        config.max_chunk_substeps,
    )
    .map_err(err)?;
    let one_partition = vec![ppo_minibatches(groups.len(), 1, 0, 0, 0)];
    let two_partitions = vec![
        one_partition[0].clone(),
        ppo_minibatches(groups.len(), 1, 0, 0, 1),
    ];
    ppo.update_groups_ppo(
        &groups,
        &targets,
        &advantages,
        &behavior_joint,
        &one_partition,
        0.2,
        0.0001,
        0.5,
        false,
        true,
        config.max_chunk_substeps,
    )
    .map_err(err)?;
    let (gae_base, gae_bytes) = state_bytes(&mut gae, &config, legacy.adam_step, 1)?;
    let (ppo_base, ppo_bytes) = state_bytes(&mut ppo, &config, legacy.adam_step, 1)?;
    let gae_hash = publish_bytes(
        &command.output_directory,
        "gae-one-pass-optimizer.json",
        &gae_bytes,
    )?;
    let ppo_hash = publish_bytes(
        &command.output_directory,
        "ppo-one-pass-optimizer.json",
        &ppo_bytes,
    )?;
    let mut update_comparison = Comparison::new();
    for (name, a, b) in [
        ("parameters", &gae_base.parameters, &ppo_base.parameters),
        (
            "first_moments",
            &gae_base.first_moments,
            &ppo_base.first_moments,
        ),
        (
            "second_moments",
            &gae_base.second_moments,
            &ppo_base.second_moments,
        ),
    ] {
        update_comparison.named(name, a, b, command.envelopes.update)?;
    }
    let mut two_hashes = Vec::new();
    let mut repeated_bytes = None;
    let mut repeat_equal = true;
    let mut reload_equal = true;
    let mut two_pass_statistics = Vec::new();
    for label in ["a", "b"] {
        let mut state = import()?;
        let observed = state
            .update_groups_ppo(
                &groups,
                &targets,
                &advantages,
                &behavior_joint,
                &two_partitions,
                0.2,
                0.0001,
                0.5,
                false,
                true,
                config.max_chunk_substeps,
            )
            .map_err(err)?;
        two_pass_statistics.push(ppo_ratio_statistics(&observed, 0.2, groups.len())?);
        let (_, bytes) = state_bytes(&mut state, &config, legacy.adam_step, 2)?;
        if let Some(first) = &repeated_bytes {
            repeat_equal &= first == &bytes;
        } else {
            repeated_bytes = Some(bytes.clone());
        }
        let (decoded_base, decoded_public) =
            public_training::snapshot::decode(&bytes).map_err(err)?;
        reload_equal &= public_training::snapshot::encode(&decoded_base, &decoded_public)
            .map_err(err)?
            == bytes;
        let mut reloaded =
            PublicDeviceTrainState::import(&decoded_base, &decoded_public, &device).map_err(err)?;
        let (_, reloaded_bytes) = state_bytes(&mut reloaded, &config, legacy.adam_step, 2)?;
        reload_equal &= reloaded_bytes == bytes;
        two_hashes.push(publish_bytes(
            &command.output_directory,
            &format!("ppo-two-pass-{label}-optimizer.json"),
            &bytes,
        )?);
    }
    let passed = gradient_comparison.outside_envelope == 0
        && update_comparison.outside_envelope == 0
        && max_log_ratio_error <= command.envelopes.max_initial_log_ratio_error
        && repeat_equal
        && reload_equal;
    let result = json!({"schema":SCHEMA,"complete":true,"passed":passed,
        "source":command.source,"collection_config":command.collection_config,"trajectories":command.trajectories,
        "execution_gpu_ordinal":command.execution_gpu_ordinal,"collector_workers":1,"collected_games":0,
        "initial_adam_step":legacy.adam_step,"initial_optimizer_sha256":initial_hash,
        "complete_parent_cuda_import_bytes_exact":true,
        "cpu_base_wrapper_behavior_score_bits_exact":true,"cpu_selected_sampler_draws_exact":true,
        "disabled_projection_parameters_moments_and_gradients_zero":true,
        "base_state_sha256":hex(&legacy.state_sha256_v1().map_err(err)?),
        "groups":groups.len(),"substeps":steps.iter().map(Vec::len).sum::<usize>(),
        "envelopes":command.envelopes,"initial_ratio_rows":ratios,"max_initial_abs_log_ratio":max_log_ratio_error,
        "gradient_comparison":gradient_comparison,"updated_base_comparison":update_comparison,
        "one_pass_optimizer_sha256":{"gae":gae_hash,"ppo":ppo_hash},"two_pass_optimizer_sha256":two_hashes,
        "two_pass_statistics":two_pass_statistics,"two_pass_repeat_bytes_exact":repeat_equal,
        "snapshot_decode_encode_and_cuda_reload_bytes_exact":reload_equal,
        "backward_full_batch_passes":8,
        "non_claim":"Saved-batch engineering only. Inherited raw-softmax p_new/p_old surrogate under quantized q collection; no exact importance-sampling, strength, end-to-end throughput, or update-boundary continuation claim."});
    publish_json(&command.output_directory, "completion.json", &result)?;
    ensure(
        passed,
        "saved-group qualification failed; see completion.json and retained diagnostic outputs",
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualification_envelope_uses_gae_reference_units_and_rejects_nonfinite() {
        let e = Envelope {
            absolute: 0.1,
            relative: 0.01,
        };
        e.validate().unwrap();
        let mut c = Comparison::new();
        c.observe("gradient", 0, 10.0, 10.125, e).unwrap();
        c.observe("gradient", 1, 0.0, 0.125, e).unwrap();
        assert_eq!(c.scalars, 2);
        assert_eq!(c.outside_envelope, 1);
        assert_eq!(c.worst_index, 1);
        assert!(c.observe("gradient", 2, 0.0, f32::NAN, e).is_err());
        for invalid in [
            Envelope {
                absolute: 0.0,
                relative: 0.0,
            },
            Envelope {
                absolute: -0.1,
                relative: 0.0,
            },
            Envelope {
                absolute: f64::INFINITY,
                relative: 0.0,
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }
}
