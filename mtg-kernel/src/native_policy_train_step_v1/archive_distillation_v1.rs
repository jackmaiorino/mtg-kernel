//! Archive distillation phase 1 on the CPU reference (collab
//! LANES/spy-archive-distillation-20261013): auxiliary-only Adam steps on
//! off-batch rows, with no ordinary loss. Each term is `c / D` times the sum
//! of its rows' forward-KL divergences, entered as explicit logit gradients
//! through the same reverse pass as the line (b) auxiliary term, and the
//! terms combine in the order given before one Adam step.
//!
//! The archive term's rows are E's supported executed choices from archived
//! search games, each with a one-hot target. The anchor term's rows are the
//! parent's own ordinary decisions, each with the parent's softmax as its
//! target, so at the parent its gradient is exactly zero. No row gives the
//! value head a gradient (`d_value` is zero for every row).

use super::line_b_auxiliary_v1::{LineBAuxiliaryResultV1, LineBAuxiliaryTermsV1};
use super::*;
use crate::line_b_teacher_target_v1::{line_b_divergence_v1, LineBDivergenceV1, LineBSoftmaxV1};
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;

/// One off-batch row: its encoded decision and frozen target.
pub(crate) struct ExternalAuxiliaryRowV1<'a> {
    pub(crate) encoded: NativeEncodedDecisionViewV1<'a>,
    pub(crate) target: &'a LineBSoftmaxV1,
}

/// `coefficient / denominator` times the sum over `rows` of the divergence in
/// `direction`. A denominator above the row count fixes each row's weight
/// for a partial batch.
pub(crate) struct ExternalAuxiliaryInputV1<'a> {
    pub(crate) direction: LineBDivergenceV1,
    pub(crate) coefficient: f64,
    pub(crate) denominator: usize,
    pub(crate) rows: Vec<ExternalAuxiliaryRowV1<'a>>,
}

/// The exact one-hot target E's supported choice CE uses
/// (`e_supported_policy_v1`): probability 1 and log-probability 0 at
/// `index`, and 0 and minus infinity elsewhere. The forward divergence reads
/// only the target's positive entries.
pub(crate) fn one_hot_target_v1(
    width: usize,
    index: usize,
) -> Result<LineBSoftmaxV1, NativePolicyTrainErrorV1> {
    if width < 2 || index >= width {
        return Err(invalid("external-auxiliary-one-hot-target"));
    }
    let mut probabilities = vec![0.0; width];
    probabilities[index] = 1.0;
    let mut log_probabilities = vec![f64::NEG_INFINITY; width];
    log_probabilities[index] = 0.0;
    Ok(LineBSoftmaxV1 {
        probabilities,
        log_probabilities,
        weight_sum: 1.0,
    })
}

fn invalid(code: &'static str) -> NativePolicyTrainErrorV1 {
    NativePolicyTrainErrorV1::LineBAuxiliary { code }
}

fn input_config_v1(
    model: &NativePolicyValueNetV1,
    generation: FreshLineageGenerationV1,
) -> NativePolicyValueModelConfigV1 {
    match generation {
        FreshLineageGenerationV1::V3 => model.feature_transfer_config_v3(),
        FreshLineageGenerationV1::V4 => model.feature_transfer_config_v4(),
    }
}

/// Each row's own forward tape at the start-of-step parameters, its logit
/// gradient `(c / D) * dKL/dz` rounded once to binary32, and the reverse pass
/// into a separate gradient buffer, in row order.
pub(super) fn external_auxiliary_terms_v1(
    input: &ExternalAuxiliaryInputV1<'_>,
    parameters: &[NativeNamedParameterV1],
    config: NativePolicyValueModelConfigV1,
) -> Result<LineBAuxiliaryTermsV1, NativePolicyTrainErrorV1> {
    if input.rows.is_empty() || input.denominator < input.rows.len() {
        return Err(invalid("external-auxiliary-denominator"));
    }
    if !(input.coefficient.is_finite() && input.coefficient >= 0.0) {
        return Err(invalid("external-auxiliary-coefficient"));
    }
    let scale = input.coefficient / input.denominator as f64;
    let mut gradients: Vec<Vec<f32>> = parameters
        .iter()
        .map(|parameter| vec![0.0; parameter.values.len()])
        .collect();
    let mut workspace = ReverseWorkspaceV1::default();
    let mut rows = Vec::with_capacity(input.rows.len());
    let mut divergences = Vec::with_capacity(input.rows.len());
    let mut loss = 0.0_f64;
    for row in &input.rows {
        let tape = forward_with_tape(parameters, config, row.encoded)?;
        let logits: Vec<f64> = tape
            .logits_v1()
            .iter()
            .map(|&logit| f64::from(logit))
            .collect();
        if logits.len() < 2 {
            return Err(invalid("external-auxiliary-forced-row"));
        }
        let value = line_b_divergence_v1(input.direction, &logits, row.target)
            .map_err(|_| invalid("external-auxiliary-divergence"))?;
        let exact: Vec<f64> = value.logit_gradient.iter().map(|g| scale * g).collect();
        let d_logits: Vec<f32> = exact.iter().map(|&g| g as f32).collect();
        reverse_decision(
            parameters,
            &mut gradients,
            &tape,
            &d_logits,
            0.0,
            &mut workspace.decision,
        )?;
        loss += scale * value.divergence;
        divergences.push(Some(value.divergence));
        rows.push((d_logits, exact));
    }
    Ok(LineBAuxiliaryTermsV1::from_parts_v1(
        input.coefficient > 0.0,
        gradients,
        rows,
        divergences,
        loss,
    ))
}

impl NativePolicyValueTrainStateV1 {
    /// One phase-1 step: the given terms alone, all over off-batch rows,
    /// with no ordinary loss. The same gauge accounting, scorer-bias pinning
    /// and Adam step as every other CPU update. Terms combine in the order
    /// given, so each term's telemetry compares it with the terms before it.
    pub(crate) fn train_step_auxiliary_only_v1(
        &mut self,
        generation: FreshLineageGenerationV1,
        learning_rate: f32,
        terms: &[&ExternalAuxiliaryInputV1<'_>],
    ) -> Result<
        (Vec<LineBAuxiliaryResultV1>, NativeScorerBiasGaugeRecordV1),
        NativePolicyTrainErrorV1,
    > {
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidLearningRate);
        }
        if terms.is_empty() {
            return Err(invalid("auxiliary-only-no-terms"));
        }
        let config = input_config_v1(&self.model, generation);
        let parameters = self.model.parameter_snapshot_v1();
        validate_parameter_manifest(&parameters)?;
        validate_optimizer_state(&parameters, &self.first_moments, &self.second_moments)?;
        validate_canonical_gauge_state(
            &parameters,
            &self.first_moments,
            &self.second_moments,
            self.scorer_bias_anchor_bits,
        )?;
        let built = terms
            .iter()
            .map(|input| external_auxiliary_terms_v1(input, &parameters, config))
            .collect::<Result<Vec<_>, _>>()?;
        let mut gradients: Vec<Vec<f32>> = parameters
            .iter()
            .map(|parameter| vec![0.0; parameter.values.len()])
            .collect();
        let mut gauge = ScorerBiasGaugeAccumulatorV1::default();
        let mut results = Vec::with_capacity(built.len());
        for terms in built {
            results.push(terms.combine_v1(&parameters, &mut gradients, &mut gauge)?);
        }
        validate_finite_nested("auxiliary_only_gradient", &gradients)?;
        let raw_scorer_bias_residual = gradients[SCORER_SECOND_BIAS][0];
        let scorer_bias_before_bits = parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        let mut scorer_bias_gauge =
            gauge.finish(raw_scorer_bias_residual, scorer_bias_before_bits)?;
        gradients[SCORER_SECOND_BIAS][0] = 0.0;
        self.commit_adam_step_v1(
            &parameters,
            &gradients,
            learning_rate,
            &mut scorer_bias_gauge,
        )?;
        Ok((results, scorer_bias_gauge))
    }

    /// Adam on `gradients` (already gauge-canonicalized), the scorer-bias
    /// anchor restored with zero moments, revalidated, then committed.
    fn commit_adam_step_v1(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        gradients: &[Vec<f32>],
        learning_rate: f32,
        scorer_bias_gauge: &mut NativeScorerBiasGaugeRecordV1,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        let next_step = self
            .adam_step
            .checked_add(1)
            .ok_or(NativePolicyTrainErrorV1::AdamStepOverflow)?;
        let (mut next_parameters, mut next_first_moments, mut next_second_moments) = adam_update(
            parameters,
            gradients,
            &self.first_moments,
            &self.second_moments,
            next_step,
            learning_rate,
        )?;
        next_parameters[SCORER_SECOND_BIAS].values[0] =
            f32::from_bits(self.scorer_bias_anchor_bits);
        next_first_moments[SCORER_SECOND_BIAS][0] = 0.0;
        next_second_moments[SCORER_SECOND_BIAS][0] = 0.0;
        scorer_bias_gauge.parameter_after_bits =
            next_parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        let mut candidate_model = self.model.clone();
        candidate_model.replace_parameter_snapshot_v1(&next_parameters)?;
        validate_optimizer_state(&next_parameters, &next_first_moments, &next_second_moments)?;
        validate_canonical_gauge_state(
            &next_parameters,
            &next_first_moments,
            &next_second_moments,
            self.scorer_bias_anchor_bits,
        )?;
        self.model = candidate_model;
        self.adam_step = next_step;
        self.first_moments = next_first_moments;
        self.second_moments = next_second_moments;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
