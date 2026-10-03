use super::*;

pub(crate) struct PublicTrainingStep<'a> {
    pub(crate) view: NativeEncodedDecisionViewV1<'a>,
    pub(crate) auxiliary: &'a PublicFeatureRowsV1,
    pub(crate) selected: usize,
    pub(crate) expected_logits: &'a [u32],
    pub(crate) expected_value: u32,
}

pub(crate) struct PublicTrainingGroup<'a> {
    pub(crate) substeps: &'a [PublicTrainingStep<'a>],
}

impl PublicDeviceTrainState {
    #[allow(
        clippy::too_many_arguments,
        reason = "preserves the explicit numerical and collection input contract"
    )]
    pub(crate) fn update_groups(
        &mut self,
        groups: &[PublicTrainingGroup<'_>],
        targets: &[f32],
        advantages: &[f32],
        learning_rate: f32,
        value_coefficient: f32,
        entropy_coefficient: f32,
        inputs_enabled: bool,
        object_inputs_enabled: bool,
        max_chunk_substeps: usize,
    ) -> Result<(), Box<dyn Error>> {
        self.validate_groups(groups, targets, advantages, max_chunk_substeps)?;
        let mut accumulator = PublicGradientAccumulator::default();
        self.accumulate_chunks(
            &mut accumulator,
            groups,
            targets,
            advantages,
            None,
            value_coefficient,
            entropy_coefficient,
            inputs_enabled,
            object_inputs_enabled,
            max_chunk_substeps,
            groups.len() as f32,
            true,
        )?;
        self.apply(accumulator, learning_rate)
    }

    /// PPO over the current batch: for each epoch's partition, one Adam step
    /// per minibatch, normalized by that minibatch's group count. Only the
    /// first minibatch of the first epoch sees the behavior parameters, so the
    /// transported-behavior envelope is checked there alone. Returns every
    /// group ratio observed, in step order, for statistics.
    #[allow(
        clippy::too_many_arguments,
        reason = "preserves the explicit numerical and collection input contract"
    )]
    pub(crate) fn update_groups_ppo(
        &mut self,
        groups: &[PublicTrainingGroup<'_>],
        targets: &[f32],
        advantages: &[f32],
        behavior_joint: &[f32],
        partitions: &[Vec<Vec<usize>>],
        clip: f32,
        learning_rate: f32,
        value_coefficient: f32,
        inputs_enabled: bool,
        object_inputs_enabled: bool,
        max_chunk_substeps: usize,
    ) -> Result<Vec<f32>, Box<dyn Error>> {
        self.validate_groups(groups, targets, advantages, max_chunk_substeps)?;
        if behavior_joint.len() != groups.len()
            || behavior_joint.iter().any(|v| !v.is_finite())
            || partitions.is_empty()
            || partitions.iter().any(|epoch| {
                let mut seen: Vec<usize> = epoch.concat();
                seen.sort_unstable();
                epoch.iter().any(Vec::is_empty) || seen != (0..groups.len()).collect::<Vec<_>>()
            })
        {
            return Err(training_error("invalid public PPO behavior or partition"));
        }
        let mut ratios = Vec::new();
        for (epoch, minibatches) in partitions.iter().enumerate() {
            for (index, minibatch) in minibatches.iter().enumerate() {
                let selected: Vec<_> = minibatch
                    .iter()
                    .map(|&g| PublicTrainingGroup {
                        substeps: groups[g].substeps,
                    })
                    .collect();
                let pick =
                    |values: &[f32]| minibatch.iter().map(|&g| values[g]).collect::<Vec<_>>();
                let mut accumulator = PublicGradientAccumulator::default();
                ratios.extend(self.accumulate_chunks(
                    &mut accumulator,
                    &selected,
                    &pick(targets),
                    &pick(advantages),
                    Some((&pick(behavior_joint), clip)),
                    value_coefficient,
                    0.0,
                    inputs_enabled,
                    object_inputs_enabled,
                    max_chunk_substeps,
                    selected.len() as f32,
                    epoch == 0 && index == 0,
                )?);
                self.apply(accumulator, learning_rate)?;
            }
        }
        Ok(ratios)
    }

    fn validate_groups(
        &self,
        groups: &[PublicTrainingGroup<'_>],
        targets: &[f32],
        advantages: &[f32],
        max_chunk_substeps: usize,
    ) -> Result<(), Box<dyn Error>> {
        use crate::native_flat_tensorizer_v4::*;
        if groups.is_empty()
            || groups.len() > 1_000_000
            || groups.len() != targets.len()
            || groups.len() != advantages.len()
            || !(1..=512).contains(&max_chunk_substeps)
            || targets.iter().chain(advantages).any(|v| !v.is_finite())
        {
            return Err(training_error("invalid public update groups or bounds"));
        }
        let config = NativePolicyValueModelConfigV1 {
            feature_schema_version: FEATURE_SCHEMA_VERSION_V4,
            feature_registry_version: FEATURE_REGISTRY_VERSION_V4,
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4,
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4,
            ..NativePolicyValueModelConfigV1::contract_v1()
        };
        for group in groups {
            if group.substeps.is_empty() || group.substeps.len() > max_chunk_substeps {
                return Err(training_error("physical group exceeds bounded chunk"));
            }
            for step in group.substeps {
                step.view.validate(config)?;
                step.auxiliary
                    .validate_tokens(step.view.object_card_ids)
                    .map_err(training_error)?;
                if step.expected_logits.len()
                    != step.view.action_features.len() / ACTION_FEATURE_DIM_V1
                    || step.selected >= step.expected_logits.len()
                    || !f32::from_bits(step.expected_value).is_finite()
                    || step
                        .expected_logits
                        .iter()
                        .any(|v| !f32::from_bits(*v).is_finite())
                {
                    return Err(training_error(
                        "public behavior outputs or selection differ",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Packs, uploads and backpropagates `groups` in bounded chunks into
    /// `accumulator`. `ppo` selects the clipped loss with these behavior joint
    /// log-probabilities; None is the grouped GAE loss. Returns PPO ratios.
    #[allow(
        clippy::too_many_arguments,
        reason = "preserves the explicit numerical and collection input contract"
    )]
    fn accumulate_chunks(
        &self,
        accumulator: &mut PublicGradientAccumulator,
        groups: &[PublicTrainingGroup<'_>],
        targets: &[f32],
        advantages: &[f32],
        ppo: Option<(&[f32], f32)>,
        value_coefficient: f32,
        entropy_coefficient: f32,
        inputs_enabled: bool,
        object_inputs_enabled: bool,
        max_chunk_substeps: usize,
        normalization_group_count: f32,
        check_behavior_envelope: bool,
    ) -> Result<Vec<f32>, Box<dyn Error>> {
        let mut ratios = Vec::new();
        let mut start = 0;
        while start < groups.len() {
            let mut end = start;
            let mut count = 0;
            while end < groups.len() && count + groups[end].substeps.len() <= max_chunk_substeps {
                count += groups[end].substeps.len();
                end += 1;
            }
            let mut views = Vec::with_capacity(count);
            let mut rows = Vec::with_capacity(count);
            let mut selected = Vec::with_capacity(count);
            let mut indices = Vec::with_capacity(count);
            let mut first = Vec::new();
            for (index, group) in groups[start..end].iter().enumerate() {
                first.push(views.len());
                for step in group.substeps {
                    views.push(step.view);
                    selected.push(step.selected);
                    indices.push(index);
                    let mut row = step.auxiliary.clone();
                    if !inputs_enabled {
                        row.state.fill(0.0);
                    }
                    if !inputs_enabled || !object_inputs_enabled {
                        for object in &mut row.objects {
                            object.fill(0.0);
                        }
                    }
                    rows.push(row);
                }
            }
            let mut host = HostPackingWorkspace::default();
            host.pack_views(&views)?;
            let batch = DevicePackedBatch::upload_feature_transfer_v3(&self.legacy.device, &host);
            let plan = build_dense_group_loss_plan_gae_v1(
                &host,
                &selected,
                &indices,
                &first,
                &targets[start..end],
                &advantages[start..end],
                &self.legacy.device,
            )?;
            let output = match ppo {
                None => self.chunk_backward_gae(
                    accumulator,
                    &batch,
                    &rows,
                    &plan,
                    value_coefficient,
                    normalization_group_count,
                    entropy_coefficient,
                )?,
                Some((behavior_joint, clip)) => {
                    let (output, chunk_ratios) = self.chunk_backward_ppo(
                        accumulator,
                        &batch,
                        &rows,
                        &plan,
                        &behavior_joint[start..end],
                        clip,
                        value_coefficient,
                        normalization_group_count,
                    )?;
                    ratios.extend(chunk_ratios);
                    output
                }
            };
            if !check_behavior_envelope {
                start = end;
                continue;
            }
            let close = |actual: f32, bits: u32| {
                actual.is_finite()
                    && (actual - f32::from_bits(bits)).abs()
                        <= 1e-3 + 1e-3 * f32::from_bits(bits).abs()
            };
            let mut offset = 0;
            let mut substep = 0;
            for group in &groups[start..end] {
                for step in group.substeps {
                    if !close(output.value_outputs[substep], step.expected_value)
                        || output.logit_outputs[offset..offset + step.expected_logits.len()]
                            .iter()
                            .zip(step.expected_logits)
                            .any(|(a, b)| !close(*a, *b))
                    {
                        return Err(training_error(
                            "CUDA public forward exceeds transported behavior envelope",
                        ));
                    }
                    offset += step.expected_logits.len();
                    substep += 1;
                }
            }
            start = end;
        }
        Ok(ratios)
    }
}
