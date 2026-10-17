use super::*;

pub(crate) struct RegistrationTrainingStep<'a> {
    pub(crate) view: NativeEncodedDecisionViewV1<'a>,
    pub(crate) auxiliary: &'a RegistrationFeatureRowsV1,
    pub(crate) expected: crate::own_registration_v1::RegistrationExpectationV1<'a>,
    pub(crate) selected: usize,
    pub(crate) expected_logits: &'a [u32],
    pub(crate) expected_value: u32,
}
pub(crate) struct RegistrationTrainingGroup<'a> {
    pub(crate) substeps: &'a [RegistrationTrainingStep<'a>],
}
impl RegistrationTrainingStep<'_> {
    pub(super) fn validate_context(&self, mode: RegistrationModeV1) -> Result<(), String> {
        self.auxiliary.validate_expected(mode, self.expected)
    }
}
impl RegistrationDeviceTrainState {
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit ordinary GAE input contract"
    )]
    pub(crate) fn update_groups(
        &mut self,
        groups: &[RegistrationTrainingGroup<'_>],
        targets: &[f32],
        advantages: &[f32],
        learning_rate: f32,
        value_coefficient: f32,
        entropy_coefficient: f32,
        max_chunk_substeps: usize,
    ) -> Result<(), Box<dyn Error>> {
        self.validate_groups(groups, targets, advantages, max_chunk_substeps)?;
        let mut accumulator = RegistrationGradientAccumulator::default();
        self.accumulate_chunks(
            &mut accumulator,
            groups,
            targets,
            advantages,
            value_coefficient,
            entropy_coefficient,
            max_chunk_substeps,
            groups.len() as f32,
        )?;
        self.apply(accumulator, learning_rate)
    }
    fn validate_groups(
        &self,
        groups: &[RegistrationTrainingGroup<'_>],
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
            return Err(training_error(
                "invalid registration update groups or bounds",
            ));
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
                step.validate_context(self.mode).map_err(training_error)?;
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
                        "registration behavior outputs or selection differ",
                    ));
                }
            }
        }
        Ok(())
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "explicit ordinary GAE input contract"
    )]
    fn accumulate_chunks(
        &self,
        accumulator: &mut RegistrationGradientAccumulator,
        groups: &[RegistrationTrainingGroup<'_>],
        targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        entropy_coefficient: f32,
        max_chunk_substeps: usize,
        normalization_group_count: f32,
    ) -> Result<(), Box<dyn Error>> {
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
                    rows.push(step.auxiliary.clone());
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
            let output = self.chunk_backward_gae(
                accumulator,
                &batch,
                &rows,
                &plan,
                value_coefficient,
                normalization_group_count,
                entropy_coefficient,
            )?;
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
                            "CUDA registration forward exceeds transported behavior envelope",
                        ));
                    }
                    offset += step.expected_logits.len();
                    substep += 1;
                }
            }
            start = end;
        }
        Ok(())
    }
}
