//! Terminal imitation plus separately normalized parent-policy forward KL.
//! No retention row carries an invented reward, value target or chosen action.
use super::*;
use crate::native_policy_anchor_v1::{
    native_policy_anchor_v1, NativePolicyAnchorGroupV1, NativePolicyAnchorRowV1,
};

#[derive(Clone, Copy)]
pub(crate) struct RetentionRowV1<'a> {
    pub encoded: NativeEncodedDecisionViewV1<'a>,
    pub expected_current_logits: &'a [u32],
    pub expected_current_value: u32,
    pub parent_logits: &'a [f32],
}

pub(crate) struct RetentionGroupV1<'a> {
    pub rows: &'a [RetentionRowV1<'a>],
}

#[derive(Debug, PartialEq)]
pub(crate) struct RetainedImitationResultV1 {
    pub teacher_loss: f32,
    pub retention_kl: f32,
    pub weighted_retention_loss: f32,
    pub loss: f32,
    pub adam_step: u64,
    pub gradients: Vec<NativeNamedParameterV1>,
    pub teacher_gauge: NativeScorerBiasGaugeRecordV1,
    pub retention_gauge: Option<NativeScorerBiasGaugeRecordV1>,
}

fn failure(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}

struct RetentionBackward {
    kl: f32,
    weighted: f32,
    gradients: Vec<Vec<f32>>,
    gauge: NativeScorerBiasGaugeRecordV1,
}

fn retention_backward(
    state: &NativePolicyValueTrainStateV1,
    groups: &[RetentionGroupV1<'_>],
    beta: f32,
    workers: usize,
) -> Result<RetentionBackward, String> {
    if groups.is_empty() || groups.iter().any(|g| g.rows.is_empty()) {
        return Err("retention requires nonempty physical groups".into());
    }
    let parameters = state.model.parameter_snapshot_v1();
    let config = state.model.feature_transfer_config_v4();
    let mut tapes = Vec::new();
    for group in groups {
        let mut rows = Vec::new();
        for row in group.rows {
            let tape = forward_with_tape(&parameters, config, row.encoded).map_err(failure)?;
            if tape.logits_v1().len() != row.expected_current_logits.len()
                || !tape
                    .logits_v1()
                    .iter()
                    .zip(row.expected_current_logits)
                    .all(|(v, b)| v.to_bits() == *b)
                || tape.value_v1().to_bits() != row.expected_current_value
            {
                return Err("retention current-score replay differs".into());
            }
            rows.push(tape);
        }
        tapes.push(rows);
    }
    let anchor_rows: Vec<Vec<_>> = groups
        .iter()
        .zip(&tapes)
        .map(|(group, t)| {
            group
                .rows
                .iter()
                .zip(t)
                .map(|(row, tape)| NativePolicyAnchorRowV1 {
                    parent_logits: row.parent_logits,
                    current_logits: tape.logits_v1(),
                    legal_action_count: tape.logits_v1().len(),
                })
                .collect()
        })
        .collect();
    let anchors: Vec<_> = anchor_rows
        .iter()
        .map(|rows| NativePolicyAnchorGroupV1 { rows })
        .collect();
    let anchor = native_policy_anchor_v1(&anchors, beta).map_err(failure)?;
    // Four fixed logical partitions, reduced in ordinal/parameter/value order.
    // Physical groups remain intact; a row-rich group is not renormalized.
    let zero = || {
        parameters
            .iter()
            .map(|p| vec![0.0; p.values.len()])
            .collect::<Vec<_>>()
    };
    let partition = |ordinal: usize| -> Result<Vec<Vec<f32>>, String> {
        let mut gradients = zero();
        let mut workspace = ReverseDecisionWorkspaceV1::default();
        let (start, end) = fixed_partition_reverse_bounds_v1(groups.len(), 4, ordinal);
        for reversed in start..end {
            let g = groups.len() - 1 - reversed;
            for row in (0..tapes[g].len()).rev() {
                reverse_decision(
                    &parameters,
                    &mut gradients,
                    &tapes[g][row],
                    &anchor.groups[g].rows[row].current_logit_gradient,
                    0.0,
                    &mut workspace,
                )
                .map_err(failure)?;
            }
        }
        Ok(gradients)
    };
    let parts = if workers == 1 {
        (0..4).map(partition).collect::<Result<Vec<_>, _>>()?
    } else {
        thread::scope(|scope| -> Result<Vec<Vec<Vec<f32>>>, String> {
            let mut handles = Vec::new();
            let per_worker = 4usize.div_ceil(workers);
            for start in (0..4).step_by(per_worker) {
                let partition = &partition;
                handles.push(
                    thread::Builder::new()
                        .stack_size(16 * 1024 * 1024)
                        .spawn_scoped(scope, move || {
                            (start..(start + per_worker).min(4))
                                .map(partition)
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .map_err(failure)?,
                );
            }
            let mut out = Vec::new();
            for handle in handles {
                out.extend(handle.join().map_err(|_| "retention worker panicked")??);
            }
            Ok(out)
        })?
    };
    let mut gradients = zero();
    for part in parts {
        for (dst, src) in gradients.iter_mut().zip(part) {
            for (a, b) in dst.iter_mut().zip(src) {
                *a += b;
            }
        }
    }
    validate_finite_nested("retention_gradient", &gradients).map_err(failure)?;
    let mut gauge = ScorerBiasGaugeAccumulatorV1::default();
    let scale = beta / exact_group_count_f32(groups.len()).map_err(failure)?;
    for group in anchor_rows.iter().rev() {
        for row in group.iter().rev() {
            // KL gradient is the sum of two opposite selected-action CE gradients:
            // scale*(current-onehot) + scale*(onehot-parent). The arbitrary selected
            // index cancels. Observe both terms to retain the existing scale bound.
            gauge
                .observe(row.current_logits, 0, -scale)
                .map_err(failure)?;
            gauge
                .observe(row.parent_logits, 0, scale)
                .map_err(failure)?;
        }
    }
    let gauge = gauge
        .finish(
            gradients[SCORER_SECOND_BIAS][0],
            state.scorer_bias_anchor_bits,
        )
        .map_err(failure)?;
    gradients[SCORER_SECOND_BIAS][0] = 0.0;
    Ok(RetentionBackward {
        kl: anchor.forward_kl,
        weighted: anchor.weighted_forward_kl,
        gradients,
        gauge,
    })
}

impl NativePolicyValueTrainStateV1 {
    /// Mean teaching CE plus beta times mean retention KL. Each mean uses its
    /// own physical-group denominator. Parent logits are frozen caller inputs.
    /// CPU engineering seam only; it is not a qualified campaign launcher.
    pub(crate) fn train_step_retained_imitation_v4(
        &mut self,
        teaching: &[NativePolicyPhysicalDecisionV1<'_>],
        retention: &[RetentionGroupV1<'_>],
        beta: f32,
        learning_rate: f32,
        workers: usize,
    ) -> Result<RetainedImitationResultV1, String> {
        if !beta.is_finite() || beta < 0.0 {
            return Err("retention beta must be finite and nonnegative".into());
        }
        if !(1..=4).contains(&workers) {
            return Err("retention worker bound is1..4".into());
        }
        if beta == 0.0 {
            let result = self
                .train_step_terminal_winner_imitation_v4(teaching, learning_rate, workers)
                .map_err(failure)?;
            return Ok(RetainedImitationResultV1 {
                teacher_loss: result.loss,
                retention_kl: 0.0,
                weighted_retention_loss: 0.0,
                loss: result.loss,
                adam_step: result.adam_step,
                gradients: result.gradients,
                teacher_gauge: result.scorer_bias_gauge,
                retention_gauge: None,
            });
        }
        let retained = retention_backward(self, retention, beta, workers)?;
        // Reuse the established teacher backward exactly. Its disposable Adam
        // candidate supplies gradients only; the combined update below starts
        // from self, so optimizer age/moments advance exactly once, not twice.
        let mut teacher_candidate = self.clone();
        let teaching_result = teacher_candidate
            .train_step_terminal_winner_imitation_v4(teaching, learning_rate, workers)
            .map_err(failure)?;
        let parameters = self.model.parameter_snapshot_v1();
        let mut gradients: Vec<Vec<f32>> = teaching_result
            .gradients
            .iter()
            .map(|p| p.values.clone())
            .collect();
        for (dst, src) in gradients.iter_mut().zip(&retained.gradients) {
            for (a, b) in dst.iter_mut().zip(src) {
                *a += b;
            }
        }
        validate_finite_nested("retained_imitation_gradient", &gradients).map_err(failure)?;
        let loss = teaching_result.loss + retained.weighted;
        finite_scalar("retained_imitation_loss", 0, loss).map_err(failure)?;
        let next_step = self.adam_step.checked_add(1).ok_or("Adam age overflow")?;
        let (mut next, mut first, mut second) = adam_update(
            &parameters,
            &gradients,
            &self.first_moments,
            &self.second_moments,
            next_step,
            learning_rate,
        )
        .map_err(failure)?;
        next[SCORER_SECOND_BIAS].values[0] = f32::from_bits(self.scorer_bias_anchor_bits);
        first[SCORER_SECOND_BIAS][0] = 0.0;
        second[SCORER_SECOND_BIAS][0] = 0.0;
        let mut model = self.model.clone();
        model
            .replace_parameter_snapshot_v1(&next)
            .map_err(failure)?;
        validate_optimizer_state(&next, &first, &second).map_err(failure)?;
        validate_canonical_gauge_state(&next, &first, &second, self.scorer_bias_anchor_bits)
            .map_err(failure)?;
        let result = RetainedImitationResultV1 {
            teacher_loss: teaching_result.loss,
            retention_kl: retained.kl,
            weighted_retention_loss: retained.weighted,
            loss,
            adam_step: next_step,
            gradients: named_state_snapshot(&parameters, &gradients),
            teacher_gauge: teaching_result.scorer_bias_gauge,
            retention_gauge: Some(retained.gauge),
        };
        self.model = model;
        self.first_moments = first;
        self.second_moments = second;
        self.adam_step = next_step;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
