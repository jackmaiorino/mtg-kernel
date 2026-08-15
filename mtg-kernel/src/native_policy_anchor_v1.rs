//! Deterministic CPU reference for a legal-action policy anchor.
//!
//! Each row contains padded parent/current logits and an explicit legal prefix.
//! The reference computes `KL(parent || current)` over that prefix only. A
//! physical group may contain several policy rows, and the returned objective
//! is normalized once by the number of physical groups, not by the number of
//! rows or legal actions. The gradient is therefore
//! `beta / physical_group_count * (pi_current - pi_parent)` on legal logits
//! and zero on padding. CUDA agreement is judged within the declared floating
//! tolerance because represented f32 softmax rows can differ from unit mass by
//! rounding error.

/// Identifies which input logit buffer failed validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePolicyAnchorSideV1 {
    Parent,
    Current,
}

/// Failure from [`native_policy_anchor_v1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativePolicyAnchorErrorV1 {
    EmptyBatch,
    EmptyPhysicalGroup {
        group_index: usize,
    },
    EmptyLegalRow {
        group_index: usize,
        row_index: usize,
    },
    NegativeBeta,
    NonFiniteBeta,
    RowWidthMismatch {
        group_index: usize,
        row_index: usize,
        parent_width: usize,
        current_width: usize,
    },
    LegalWidthExceedsRow {
        group_index: usize,
        row_index: usize,
        legal_action_count: usize,
        row_width: usize,
    },
    NonFiniteLogit {
        group_index: usize,
        row_index: usize,
        side: NativePolicyAnchorSideV1,
        logit_index: usize,
    },
    InvalidSoftmax {
        group_index: usize,
        row_index: usize,
        side: NativePolicyAnchorSideV1,
    },
    NonFiniteForwardKl {
        group_index: usize,
        row_index: usize,
    },
    NonFiniteGradient {
        group_index: usize,
        row_index: usize,
        logit_index: usize,
    },
    NonFiniteResult,
    GroupCountNotExactlyRepresentable {
        group_count: usize,
    },
}

/// One padded policy row. Only `[..legal_action_count]` participates in the
/// softmax, KL, and gradient. The parent and current buffers must have the
/// same width so the returned gradient can be passed directly to a padded
/// CUDA row buffer.
#[derive(Clone, Copy, Debug)]
pub struct NativePolicyAnchorRowV1<'a> {
    pub parent_logits: &'a [f32],
    pub current_logits: &'a [f32],
    pub legal_action_count: usize,
}

/// One physical decision. All rows in this slice contribute to this group's
/// KL sum, while the outer batch contributes one normalization unit.
#[derive(Clone, Copy, Debug)]
pub struct NativePolicyAnchorGroupV1<'a> {
    pub rows: &'a [NativePolicyAnchorRowV1<'a>],
}

/// Per-row result. `forward_kl` is unnormalized, while `current_logit_gradient`
/// already includes beta and physical-group normalization.
#[derive(Clone, Debug, PartialEq)]
pub struct NativePolicyAnchorRowResultV1 {
    pub forward_kl: f32,
    pub current_logit_gradient: Vec<f32>,
}

/// Per-physical-group result. The group KL is unnormalized so callers can
/// inspect the exact physical grouping used by the reduction.
#[derive(Clone, Debug, PartialEq)]
pub struct NativePolicyAnchorGroupResultV1 {
    pub forward_kl: f32,
    pub rows: Vec<NativePolicyAnchorRowResultV1>,
}

/// Result of one deterministic CPU anchor evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct NativePolicyAnchorResultV1 {
    /// Mean legal-action forward KL over physical groups.
    pub forward_kl: f32,
    /// `beta * forward_kl`, matching the gradient scale.
    pub weighted_forward_kl: f32,
    pub physical_group_count: usize,
    pub groups: Vec<NativePolicyAnchorGroupResultV1>,
}

/// Computes legal-action `KL(parent || current)` and its current-logit
/// gradient for a batch of physical groups.
///
/// Inputs are validated before any result is returned. `beta` must be finite
/// and non-negative, groups and rows must be non-empty, each legal prefix must
/// be non-empty, and every stored logit, including padding, must be finite.
/// Padding is validated but never normalized, included in KL, or given a
/// nonzero gradient.
pub fn native_policy_anchor_v1(
    groups: &[NativePolicyAnchorGroupV1<'_>],
    beta: f32,
) -> Result<NativePolicyAnchorResultV1, NativePolicyAnchorErrorV1> {
    if groups.is_empty() {
        return Err(NativePolicyAnchorErrorV1::EmptyBatch);
    }
    if !beta.is_finite() {
        return Err(NativePolicyAnchorErrorV1::NonFiniteBeta);
    }
    if beta < 0.0 {
        return Err(NativePolicyAnchorErrorV1::NegativeBeta);
    }
    let physical_group_count = groups.len();
    let group_count = exact_group_count_f32(physical_group_count)?;
    let scale = beta / group_count;

    let mut result_groups = Vec::with_capacity(groups.len());
    let mut total_forward_kl = 0.0f32;

    for (group_index, group) in groups.iter().enumerate() {
        if group.rows.is_empty() {
            return Err(NativePolicyAnchorErrorV1::EmptyPhysicalGroup { group_index });
        }
        let mut group_result_rows = Vec::with_capacity(group.rows.len());
        let mut group_forward_kl = 0.0f32;

        for (row_index, row) in group.rows.iter().enumerate() {
            let parent_width = row.parent_logits.len();
            let current_width = row.current_logits.len();
            if row.legal_action_count == 0 {
                return Err(NativePolicyAnchorErrorV1::EmptyLegalRow {
                    group_index,
                    row_index,
                });
            }
            if parent_width != current_width {
                return Err(NativePolicyAnchorErrorV1::RowWidthMismatch {
                    group_index,
                    row_index,
                    parent_width,
                    current_width,
                });
            }
            if row.legal_action_count > parent_width {
                return Err(NativePolicyAnchorErrorV1::LegalWidthExceedsRow {
                    group_index,
                    row_index,
                    legal_action_count: row.legal_action_count,
                    row_width: parent_width,
                });
            }
            validate_logits(
                row.parent_logits,
                group_index,
                row_index,
                NativePolicyAnchorSideV1::Parent,
            )?;
            validate_logits(
                row.current_logits,
                group_index,
                row_index,
                NativePolicyAnchorSideV1::Current,
            )?;

            let parent = stable_softmax(
                &row.parent_logits[..row.legal_action_count],
                group_index,
                row_index,
                NativePolicyAnchorSideV1::Parent,
            )?;
            let current = stable_softmax(
                &row.current_logits[..row.legal_action_count],
                group_index,
                row_index,
                NativePolicyAnchorSideV1::Current,
            )?;

            let mut row_forward_kl = 0.0f32;
            let mut gradient = vec![0.0f32; parent_width];
            for (action_index, gradient_slot) in
                gradient.iter_mut().enumerate().take(row.legal_action_count)
            {
                let term =
                    parent.1[action_index] * (parent.0[action_index] - current.0[action_index]);
                if !term.is_finite() {
                    return Err(NativePolicyAnchorErrorV1::NonFiniteForwardKl {
                        group_index,
                        row_index,
                    });
                }
                row_forward_kl += term;
                if !row_forward_kl.is_finite() {
                    return Err(NativePolicyAnchorErrorV1::NonFiniteForwardKl {
                        group_index,
                        row_index,
                    });
                }

                let action_gradient = if beta == 0.0 {
                    0.0
                } else {
                    scale * (current.1[action_index] - parent.1[action_index])
                };
                if !action_gradient.is_finite() {
                    return Err(NativePolicyAnchorErrorV1::NonFiniteGradient {
                        group_index,
                        row_index,
                        logit_index: action_index,
                    });
                }
                *gradient_slot = action_gradient;
            }

            group_forward_kl += row_forward_kl;
            if !group_forward_kl.is_finite() {
                return Err(NativePolicyAnchorErrorV1::NonFiniteForwardKl {
                    group_index,
                    row_index,
                });
            }
            group_result_rows.push(NativePolicyAnchorRowResultV1 {
                forward_kl: row_forward_kl,
                current_logit_gradient: gradient,
            });
        }

        total_forward_kl += group_forward_kl;
        if !total_forward_kl.is_finite() {
            return Err(NativePolicyAnchorErrorV1::NonFiniteResult);
        }
        result_groups.push(NativePolicyAnchorGroupResultV1 {
            forward_kl: group_forward_kl,
            rows: group_result_rows,
        });
    }

    let forward_kl = total_forward_kl / group_count;
    let weighted_forward_kl = beta * forward_kl;
    if !forward_kl.is_finite() || !weighted_forward_kl.is_finite() {
        return Err(NativePolicyAnchorErrorV1::NonFiniteResult);
    }
    Ok(NativePolicyAnchorResultV1 {
        forward_kl,
        weighted_forward_kl,
        physical_group_count,
        groups: result_groups,
    })
}

/// Stable legal-action probabilities used to freeze one parent target row
/// before the candidate CUDA graph is built. This is deliberately the same
/// softmax implementation as the CPU KL oracle above.
pub(crate) fn native_policy_anchor_probabilities_v1(
    logits: &[f32],
) -> Result<Vec<f32>, NativePolicyAnchorErrorV1> {
    if logits.is_empty() {
        return Err(NativePolicyAnchorErrorV1::EmptyLegalRow {
            group_index: 0,
            row_index: 0,
        });
    }
    validate_logits(logits, 0, 0, NativePolicyAnchorSideV1::Parent)?;
    stable_softmax(logits, 0, 0, NativePolicyAnchorSideV1::Parent)
        .map(|(_, probabilities)| probabilities)
}

fn validate_logits(
    logits: &[f32],
    group_index: usize,
    row_index: usize,
    side: NativePolicyAnchorSideV1,
) -> Result<(), NativePolicyAnchorErrorV1> {
    for (logit_index, logit) in logits.iter().copied().enumerate() {
        if !logit.is_finite() {
            return Err(NativePolicyAnchorErrorV1::NonFiniteLogit {
                group_index,
                row_index,
                side,
                logit_index,
            });
        }
    }
    Ok(())
}

fn stable_softmax(
    logits: &[f32],
    group_index: usize,
    row_index: usize,
    side: NativePolicyAnchorSideV1,
) -> Result<(Vec<f32>, Vec<f32>), NativePolicyAnchorErrorV1> {
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut exponential_sum = 0.0f32;
    for logit in logits.iter().copied() {
        let exponential = (logit - maximum).exp();
        exponential_sum += exponential;
        if !exponential_sum.is_finite() {
            return Err(NativePolicyAnchorErrorV1::InvalidSoftmax {
                group_index,
                row_index,
                side,
            });
        }
    }
    if !exponential_sum.is_finite() || exponential_sum <= 0.0 {
        return Err(NativePolicyAnchorErrorV1::InvalidSoftmax {
            group_index,
            row_index,
            side,
        });
    }
    let log_sum = exponential_sum.ln();
    let mut log_probabilities = Vec::with_capacity(logits.len());
    let mut probabilities = Vec::with_capacity(logits.len());
    for logit in logits.iter().copied() {
        let log_probability = (logit - maximum) - log_sum;
        let probability = log_probability.exp();
        if !log_probability.is_finite() || !probability.is_finite() {
            return Err(NativePolicyAnchorErrorV1::InvalidSoftmax {
                group_index,
                row_index,
                side,
            });
        }
        log_probabilities.push(log_probability);
        probabilities.push(probability);
    }
    Ok((log_probabilities, probabilities))
}

fn exact_group_count_f32(group_count: usize) -> Result<f32, NativePolicyAnchorErrorV1> {
    let represented = group_count as f32;
    if !represented.is_finite() || represented as u128 != group_count as u128 {
        return Err(NativePolicyAnchorErrorV1::GroupCountNotExactlyRepresentable { group_count });
    }
    Ok(represented)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row<'a>(
        parent_logits: &'a [f32],
        current_logits: &'a [f32],
        legal_action_count: usize,
    ) -> NativePolicyAnchorRowV1<'a> {
        NativePolicyAnchorRowV1 {
            parent_logits,
            current_logits,
            legal_action_count,
        }
    }

    fn group<'a>(rows: &'a [NativePolicyAnchorRowV1<'a>]) -> NativePolicyAnchorGroupV1<'a> {
        NativePolicyAnchorGroupV1 { rows }
    }

    #[test]
    fn hand_arithmetic_matches_forward_kl_and_gradient() {
        let parent = [0.0, 0.0];
        let current = [0.0, 1.0];
        let rows = [row(&parent, &current, 2)];
        let groups = [group(&rows)];
        let result = native_policy_anchor_v1(&groups, 2.0).unwrap();

        let expected_parent = 0.5f32;
        let expected_current0 = 1.0 / (1.0 + 1.0f32.exp());
        let expected_current1 = 1.0 - expected_current0;
        let expected_kl = expected_parent * ((0.5f32).ln() - expected_current0.ln())
            + expected_parent * ((0.5f32).ln() - expected_current1.ln());
        assert!((result.forward_kl - expected_kl).abs() < 1.0e-6);
        assert!((result.weighted_forward_kl - 2.0 * expected_kl).abs() < 1.0e-6);
        let gradients = &result.groups[0].rows[0].current_logit_gradient;
        assert!((gradients[0] - 2.0 * (expected_current0 - 0.5)).abs() < 1.0e-6);
        assert!((gradients[1] - 2.0 * (expected_current1 - 0.5)).abs() < 1.0e-6);
    }

    #[test]
    fn matching_distributions_have_zero_kl_and_gradient() {
        let logits = [3.0, -2.0, 0.5];
        let rows = [row(&logits, &logits, logits.len())];
        let groups = [group(&rows)];
        let result = native_policy_anchor_v1(&groups, 0.75).unwrap();
        assert_eq!(result.forward_kl.to_bits(), 0.0f32.to_bits());
        assert_eq!(result.weighted_forward_kl.to_bits(), 0.0f32.to_bits());
        assert!(result.groups[0].rows[0]
            .current_logit_gradient
            .iter()
            .all(|value| value.to_bits() == 0.0f32.to_bits()));
    }

    #[test]
    fn singleton_legal_action_is_exact_zero() {
        let parent = [100.0, 901.0];
        let current = [-77.0, -3.0];
        let rows = [row(&parent, &current, 1)];
        let groups = [group(&rows)];
        let result = native_policy_anchor_v1(&groups, 4.0).unwrap();
        assert_eq!(result.forward_kl.to_bits(), 0.0f32.to_bits());
        assert_eq!(
            result.groups[0].rows[0].current_logit_gradient,
            vec![0.0, 0.0]
        );
    }

    #[test]
    fn legal_row_widths_are_independent_and_padding_is_excluded() {
        let parent0 = [0.0, 0.0, 10_000.0, -10_000.0];
        let current0 = [1.0, 0.0, -10_000.0, 10_000.0];
        let parent1 = [0.0, 0.0, 0.0];
        let current1 = [0.0, 1.0, 0.0];
        let rows = [row(&parent0, &current0, 2), row(&parent1, &current1, 2)];
        let groups = [group(&rows)];
        let result = native_policy_anchor_v1(&groups, 1.0).unwrap();
        assert_eq!(result.groups[0].rows[0].current_logit_gradient.len(), 4);
        assert_eq!(result.groups[0].rows[1].current_logit_gradient.len(), 3);
        assert_eq!(
            result.groups[0].rows[0].current_logit_gradient[2..],
            [0.0, 0.0]
        );
        assert_eq!(result.groups[0].rows[1].current_logit_gradient[2], 0.0);
        assert!(result.groups[0].rows[0].forward_kl.is_finite());
        assert!(result.groups[0].rows[1].forward_kl.is_finite());
    }

    #[test]
    fn beta_zero_keeps_valid_inputs_and_returns_zero_weighted_objective() {
        let parent = [0.0, 4.0];
        let current = [3.0, -1.0];
        let rows = [row(&parent, &current, 2)];
        let groups = [group(&rows)];
        let result = native_policy_anchor_v1(&groups, 0.0).unwrap();
        assert!(result.forward_kl > 0.0);
        assert_eq!(result.weighted_forward_kl.to_bits(), 0.0f32.to_bits());
        assert!(result.groups[0].rows[0]
            .current_logit_gradient
            .iter()
            .all(|value| value.to_bits() == 0.0f32.to_bits()));
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert_eq!(
            native_policy_anchor_v1(&[], 1.0),
            Err(NativePolicyAnchorErrorV1::EmptyBatch)
        );
        let rows = [row(&[0.0], &[0.0], 1)];
        let empty_group = NativePolicyAnchorGroupV1 { rows: &[] };
        assert!(matches!(
            native_policy_anchor_v1(&[empty_group], 1.0),
            Err(NativePolicyAnchorErrorV1::EmptyPhysicalGroup { .. })
        ));
        assert!(matches!(
            native_policy_anchor_v1(&[group(&rows)], f32::NAN),
            Err(NativePolicyAnchorErrorV1::NonFiniteBeta)
        ));
        assert!(matches!(
            native_policy_anchor_v1(&[group(&rows)], -1.0),
            Err(NativePolicyAnchorErrorV1::NegativeBeta)
        ));
        let bad_width = [row(&[0.0, 1.0], &[0.0], 1)];
        assert!(matches!(
            native_policy_anchor_v1(&[group(&bad_width)], 1.0),
            Err(NativePolicyAnchorErrorV1::RowWidthMismatch { .. })
        ));
        let bad_legal_width = [row(&[0.0], &[0.0], 2)];
        assert!(matches!(
            native_policy_anchor_v1(&[group(&bad_legal_width)], 1.0),
            Err(NativePolicyAnchorErrorV1::LegalWidthExceedsRow { .. })
        ));
        let bad_value = [row(&[f32::NAN], &[0.0], 1)];
        assert!(matches!(
            native_policy_anchor_v1(&[group(&bad_value)], 1.0),
            Err(NativePolicyAnchorErrorV1::NonFiniteLogit { .. })
        ));
    }

    #[test]
    fn normalization_is_once_per_physical_group() {
        let parent0 = [0.0, 0.0];
        let current0 = [0.0, 1.0];
        let parent1 = [1.0, 0.0];
        let current1 = [0.0, 0.0];
        let rows0 = [row(&parent0, &current0, 2), row(&parent1, &current1, 2)];
        let rows1 = [row(&parent0, &current0, 2)];
        let groups = [group(&rows0), group(&rows1)];
        let result = native_policy_anchor_v1(&groups, 1.0).unwrap();
        let one_group_result = native_policy_anchor_v1(&[group(&rows1)], 1.0).unwrap();
        let first_group = result.groups[0].forward_kl;
        let second_group = result.groups[1].forward_kl;
        assert!((result.forward_kl - (first_group + second_group) / 2.0).abs() < 1.0e-7);
        let first_row_gradient = &result.groups[0].rows[0].current_logit_gradient;
        let one_group_gradient = &one_group_result.groups[0].rows[0].current_logit_gradient;
        assert!((first_row_gradient[0] * 2.0 - one_group_gradient[0]).abs() < 1.0e-7);
        assert!((first_row_gradient[1] * 2.0 - one_group_gradient[1]).abs() < 1.0e-7);
    }

    #[test]
    fn frozen_target_probability_helper_matches_oracle_softmax() {
        let logits = [2.0, -1.0, 0.5];
        let probabilities = native_policy_anchor_probabilities_v1(&logits).unwrap();
        let rows = [row(&logits, &logits, logits.len())];
        let groups = [group(&rows)];
        let result = native_policy_anchor_v1(&groups, 1.0).unwrap();
        assert_eq!(result.forward_kl.to_bits(), 0.0f32.to_bits());
        assert_eq!(probabilities.len(), logits.len());
        assert!((probabilities.iter().sum::<f32>() - 1.0).abs() <= 1.0e-6);
    }
}
