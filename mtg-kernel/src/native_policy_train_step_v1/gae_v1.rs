//! CPU `"gae_advantage_value/v1"` objective (design section 2): sibling to
//! `train_step_with_input_config_v1`, reusing its exact forward loop,
//! backward (Sequential and FixedPartitions), and Adam machinery. The only
//! change is the source of the per-group advantage/value-target: instead of
//! deriving them in-loop from `terminal_return`, this takes them as
//! precomputed, already-normalized inputs (`value_targets`, `advantages`),
//! one pair per group in the same order, computed once per update by
//! `expanded_deck_training_v1::compute_gae_targets_v1` from every group's
//! bit-exact `expected_value_bits` and the episode boundary both
//! preparation paths now carry. `terminal_return` itself is not read for
//! arithmetic here; it still flows into `physical_terms` unchanged, exactly
//! as diagnostic evidence.
//!
//! Like `weighted_v3`, this module supports only
//! `NativePolicyForwardInputV1::Encoded` (`expanded_deck_training_v1`'s CPU
//! dispatch never constructs `Packed`), so there is no parallel independent
//! packed recompute here, unlike the unweighted path's full generality.
//! Unlike `weighted_v3`, the loss divides by `group_count` (design section
//! 2, "the same group_count denominator idiom already used at :1833"), not
//! a per-group weight product, so `advantages` must already be normalized
//! by the caller.

use super::*;

/// Where an update's auxiliary term reads its rows: the GAE batch's own
/// groups (line (b), SQ1) or off-batch rows with their own forward tapes
/// (TP1 Stage B searched-game roots).
#[derive(Clone, Copy)]
pub(crate) enum AuxiliaryTermsSourceV1<'a> {
    InBatch(&'a LineBAuxiliaryInputV1),
    External(&'a archive_distillation_v1::ExternalAuxiliaryInputV1<'a>),
}

fn validate_gae_inputs_v1(
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_targets: &[f32],
    advantages: &[f32],
) -> Result<(), NativePolicyTrainErrorV1> {
    if groups.is_empty() {
        return Err(NativePolicyTrainErrorV1::EmptyBatch);
    }
    if groups.len() != value_targets.len() || groups.len() != advantages.len() {
        return Err(NativePolicyTrainErrorV1::GaeGroupCountMismatch {
            groups: groups.len(),
            value_targets: value_targets.len(),
            advantages: advantages.len(),
        });
    }
    for (group_index, group) in groups.iter().enumerate() {
        if group.substeps.is_empty() {
            return Err(NativePolicyTrainErrorV1::EmptyPhysicalDecision { group_index });
        }
        if !matches!(group.terminal_return, -1..=1) {
            return Err(NativePolicyTrainErrorV1::InvalidTerminalReturn {
                group_index,
                value: group.terminal_return,
            });
        }
        if group.baseline_bits != 0 {
            return Err(NativePolicyTrainErrorV1::BaselineUnsupportedBackend { group_index });
        }
        for (substep_index, substep) in group.substeps.iter().enumerate() {
            if !matches!(&substep.forward, NativePolicyForwardInputV1::Encoded(_)) {
                return Err(
                    NativePolicyTrainErrorV1::FeatureTransferRequiresCanonicalInput {
                        group_index,
                        substep_index,
                    },
                );
            }
        }
    }
    for (index, value) in value_targets.iter().enumerate() {
        if !value.is_finite() {
            return Err(NativePolicyTrainErrorV1::NonFinite {
                stage: "gae_value_target",
                index,
            });
        }
    }
    for (index, value) in advantages.iter().enumerate() {
        if !value.is_finite() {
            return Err(NativePolicyTrainErrorV1::NonFinite {
                stage: "gae_advantage",
                index,
            });
        }
    }
    Ok(())
}

impl NativePolicyValueTrainStateV1 {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn train_step_gae_with_e_config_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        workers: Option<usize>,
        config: NativePolicyValueModelConfigV1,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        self.train_step_gae_with_input_config_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            workers.map_or(BackwardExecutionV1::Sequential, |worker_limit| {
                BackwardExecutionV1::FixedPartitions { worker_limit }
            }),
            config,
            false,
        )
    }

    /// Line (b) CPU update: the GAE objective plus the auxiliary term,
    /// combined before the single Adam step. `backward_worker_limit` selects
    /// the fixed-partition backward (`None`: sequential), as the V4 wrappers do.
    #[allow(
        clippy::too_many_arguments,
        reason = "keeps the existing explicit input contract"
    )]
    pub(crate) fn train_step_gae_line_b_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: Option<usize>,
        input_config: NativePolicyValueModelConfigV1,
        line_b: &LineBAuxiliaryInputV1,
    ) -> Result<(NativePolicyTrainStepResultV1, LineBAuxiliaryResultV1), NativePolicyTrainErrorV1>
    {
        let backward_execution = match backward_worker_limit {
            None => BackwardExecutionV1::Sequential,
            Some(worker_limit) => BackwardExecutionV1::FixedPartitions { worker_limit },
        };
        let (result, auxiliary, _) = self.train_step_gae_core_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_execution,
            input_config,
            false,
            Some(AuxiliaryTermsSourceV1::InBatch(line_b)),
            None,
        )?;
        let auxiliary = auxiliary.ok_or(NativePolicyTrainErrorV1::LineBAuxiliary {
            code: "line-b-auxiliary-missing-result",
        })?;
        Ok((result, auxiliary))
    }

    /// TP1 Stage B CPU update (collab LANES/teacher-price-20261019,
    /// STAGE-B.md): the GAE objective plus an auxiliary term over off-batch
    /// rows (searched-game roots, never part of the GAE batch), combined
    /// before the single Adam step. The rows take no policy or value loss and
    /// do not enter the GAE group mean. Callers with no rows use the
    /// ordinary update, so it stays bit-identical.
    #[allow(
        clippy::too_many_arguments,
        reason = "keeps the existing explicit input contract"
    )]
    pub(crate) fn train_step_gae_external_auxiliary_v1(
        &mut self,
        generation: crate::sideboard_play_policy_v1::FreshLineageGenerationV1,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: Option<usize>,
        external: &archive_distillation_v1::ExternalAuxiliaryInputV1<'_>,
    ) -> Result<(NativePolicyTrainStepResultV1, LineBAuxiliaryResultV1), NativePolicyTrainErrorV1>
    {
        let input_config = match generation {
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3 => {
                self.model.feature_transfer_config_v3()
            }
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V4 => {
                self.model.feature_transfer_config_v4()
            }
        };
        let backward_execution = match backward_worker_limit {
            None => BackwardExecutionV1::Sequential,
            Some(worker_limit) => BackwardExecutionV1::FixedPartitions { worker_limit },
        };
        let (result, auxiliary, _) = self.train_step_gae_core_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_execution,
            input_config,
            false,
            Some(AuxiliaryTermsSourceV1::External(external)),
            None,
        )?;
        let auxiliary = auxiliary.ok_or(NativePolicyTrainErrorV1::LineBAuxiliary {
            code: "external-auxiliary-missing-result",
        })?;
        Ok((result, auxiliary))
    }

    /// The line (b) entry for an update batch of either fresh-lineage
    /// generation (the model's input configs are private to this module).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn train_step_gae_feature_transfer_line_b_v1(
        &mut self,
        generation: crate::sideboard_play_policy_v1::FreshLineageGenerationV1,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: Option<usize>,
        line_b: &LineBAuxiliaryInputV1,
    ) -> Result<(NativePolicyTrainStepResultV1, LineBAuxiliaryResultV1), NativePolicyTrainErrorV1>
    {
        let input_config = match generation {
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3 => {
                self.model.feature_transfer_config_v3()
            }
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V4 => {
                self.model.feature_transfer_config_v4()
            }
        };
        self.train_step_gae_line_b_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_worker_limit,
            input_config,
            line_b,
        )
    }

    /// Recovery is intentionally restricted to the qualified sequential CPU path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn train_step_gae_recovery_v1(
        &mut self,
        generation: crate::sideboard_play_policy_v1::FreshLineageGenerationV1,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        beta: f32,
        update: u64,
        diagnostics: bool,
    ) -> Result<(NativePolicyTrainStepResultV1, serde_json::Value), NativePolicyTrainErrorV1> {
        let config = match generation {
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3 => {
                self.model.feature_transfer_config_v3()
            }
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V4 => {
                self.model.feature_transfer_config_v4()
            }
        };
        let (result, _, report) = self.train_step_gae_core_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::Sequential,
            config,
            false,
            None,
            Some((beta, update, diagnostics)),
        )?;
        Ok((result, report.expect("recovery report requested")))
    }

    /// Canonical CPU GAE update under the flat-V3 input contract.
    pub(crate) fn train_step_gae_feature_transfer_v3(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        self.train_step_gae_feature_transfer_v3_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
        )
    }

    pub(crate) fn train_step_gae_feature_transfer_v4(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        self.train_step_gae_feature_transfer_v4_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
        )
    }

    pub(crate) fn train_step_gae_feature_transfer_v4_fixed_partition_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: usize,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        self.train_step_gae_feature_transfer_v4_fixed_partition_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_worker_limit,
        )
    }

    pub(crate) fn train_step_terminal_winner_imitation_v4(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        learning_rate: f32,
        backward_worker_limit: usize,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        self.train_step_terminal_winner_imitation_v4_dims_v1(
            groups,
            learning_rate,
            backward_worker_limit,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "keeps the existing explicit input contract"
    )]
    fn train_step_gae_with_input_config_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_execution: BackwardExecutionV1,
        input_config: NativePolicyValueModelConfigV1,
        imitation: bool,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        self.train_step_gae_with_input_config_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_execution,
            input_config,
            imitation,
        )
    }

    /// Profile the actual CPU GAE route without changing its arithmetic.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn train_step_gae_feature_transfer_profiled_v1(
        &mut self,
        generation: crate::sideboard_play_policy_v1::FreshLineageGenerationV1,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: Option<usize>,
        forward_workers: usize,
        recorder: &mut NativeTrainingPhaseRecorderV1<'_>,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        self.train_step_gae_feature_transfer_profiled_dims_v1(
            generation,
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_worker_limit,
            forward_workers,
            recorder,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn train_step_gae_core_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_execution: BackwardExecutionV1,
        input_config: NativePolicyValueModelConfigV1,
        imitation: bool,
        line_b: Option<AuxiliaryTermsSourceV1<'_>>,
        exploration: Option<(f32, u64, bool)>,
    ) -> Result<
        (
            NativePolicyTrainStepResultV1,
            Option<LineBAuxiliaryResultV1>,
            Option<serde_json::Value>,
        ),
        NativePolicyTrainErrorV1,
    > {
        self.train_step_gae_core_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_execution,
            input_config,
            imitation,
            line_b,
            exploration,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn train_step_gae_core_profiled_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_execution: BackwardExecutionV1,
        input_config: NativePolicyValueModelConfigV1,
        imitation: bool,
        line_b: Option<AuxiliaryTermsSourceV1<'_>>,
        exploration: Option<(f32, u64, bool)>,
        forward_workers: usize,
        phase_recorder: &mut NativeTrainingPhaseRecorderV1<'_>,
    ) -> Result<
        (
            NativePolicyTrainStepResultV1,
            Option<LineBAuxiliaryResultV1>,
            Option<serde_json::Value>,
        ),
        NativePolicyTrainErrorV1,
    > {
        self.train_step_gae_core_profiled_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_execution,
            input_config,
            imitation,
            line_b,
            exploration,
            forward_workers,
            phase_recorder,
        )
    }
}

/// The GAE CPU update at the dims `D`; the narrow names above are these at
/// `Net8DimsV1`. Line (b), external auxiliary, recovery, packed inputs, the
/// no-state-digest contract and the portable math are net-8 only (design
/// section 2.1).
impl<D: NetDimsV1> NativePolicyValueTrainStateDimsV1<D> {
    /// Canonical CPU GAE update under the flat-V3 input contract. V3
    /// sibling of `train_step_gae_feature_transfer_v4`.
    pub(crate) fn train_step_gae_feature_transfer_v3_dims_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v3();
        self.train_step_gae_with_input_config_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::Sequential,
            input_config,
            false,
        )
    }

    /// V4 sibling of `train_step_gae_feature_transfer_v3`, for the
    /// fresh-lineage (V7 observation-schema) successor contract.
    pub(crate) fn train_step_gae_feature_transfer_v4_dims_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v4();
        self.train_step_gae_with_input_config_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::Sequential,
            input_config,
            false,
        )
    }

    /// Config-driven fixed-partition sibling of
    /// `train_step_gae_feature_transfer_v4`, mirroring
    /// `train_step_feature_transfer_v4_fixed_partition_v1`. No V3 sibling
    /// exists; callers must reject any non-sequential value for a
    /// V3-generation batch before reaching this function, same as the
    /// unweighted and weighted paths.
    pub(crate) fn train_step_gae_feature_transfer_v4_fixed_partition_dims_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: usize,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v4();
        self.train_step_gae_with_input_config_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::FixedPartitions {
                worker_limit: backward_worker_limit,
            },
            input_config,
            false,
        )
    }

    /// Explicit supervised imitation seam. Unit coefficients give mean
    /// selected-action cross entropy; no value target is fitted. This does not
    /// relax the positive value-coefficient contract of any GAE entry point.
    pub(crate) fn train_step_terminal_winner_imitation_v4_dims_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        learning_rate: f32,
        backward_worker_limit: usize,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v4();
        self.train_step_gae_with_input_config_dims_v1(
            groups,
            &vec![0.0; groups.len()],
            &vec![1.0; groups.len()],
            0.0,
            learning_rate,
            BackwardExecutionV1::FixedPartitions {
                worker_limit: backward_worker_limit,
            },
            input_config,
            true,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "keeps the existing explicit input contract"
    )]
    fn train_step_gae_with_input_config_dims_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_execution: BackwardExecutionV1,
        input_config: NativePolicyValueModelConfigV1,
        imitation: bool,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        self.train_step_gae_core_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_execution,
            input_config,
            imitation,
            None,
            None,
        )
        .map(|(result, _, _)| result)
    }

    /// Profile the actual CPU GAE route without changing its arithmetic.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn train_step_gae_feature_transfer_profiled_dims_v1(
        &mut self,
        generation: crate::sideboard_play_policy_v1::FreshLineageGenerationV1,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: Option<usize>,
        forward_workers: usize,
        recorder: &mut NativeTrainingPhaseRecorderV1<'_>,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let config = match generation {
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3 => {
                self.model.feature_transfer_config_v3()
            }
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V4 => {
                self.model.feature_transfer_config_v4()
            }
        };
        let execution = match backward_worker_limit {
            None => BackwardExecutionV1::Sequential,
            Some(worker_limit) => BackwardExecutionV1::FixedPartitions { worker_limit },
        };
        self.train_step_gae_core_profiled_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            execution,
            config,
            false,
            None,
            None,
            forward_workers,
            recorder,
        )
        .map(|(result, _, _)| result)
    }

    #[allow(clippy::too_many_arguments)]
    fn train_step_gae_core_dims_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_execution: BackwardExecutionV1,
        input_config: NativePolicyValueModelConfigV1,
        imitation: bool,
        line_b: Option<AuxiliaryTermsSourceV1<'_>>,
        exploration: Option<(f32, u64, bool)>,
    ) -> Result<
        (
            NativePolicyTrainStepResultV1,
            Option<LineBAuxiliaryResultV1>,
            Option<serde_json::Value>,
        ),
        NativePolicyTrainErrorV1,
    > {
        self.train_step_gae_core_profiled_dims_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_execution,
            input_config,
            imitation,
            line_b,
            exploration,
            1,
            &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn train_step_gae_core_profiled_dims_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_execution: BackwardExecutionV1,
        input_config: NativePolicyValueModelConfigV1,
        imitation: bool,
        line_b: Option<AuxiliaryTermsSourceV1<'_>>,
        exploration: Option<(f32, u64, bool)>,
        forward_workers: usize,
        phase_recorder: &mut NativeTrainingPhaseRecorderV1<'_>,
    ) -> Result<
        (
            NativePolicyTrainStepResultV1,
            Option<LineBAuxiliaryResultV1>,
            Option<serde_json::Value>,
        ),
        NativePolicyTrainErrorV1,
    > {
        // The wide refusal list (design section 2.1), before any forward work.
        if !is_net8_dims_v1::<D>() {
            if let Some(source) = line_b {
                refuse_for_architecture_v1::<D>(match source {
                    AuxiliaryTermsSourceV1::InBatch(_) => "in-batch line (b)",
                    AuxiliaryTermsSourceV1::External(_) => "external auxiliary",
                })?;
            }
            if exploration.is_some() {
                refuse_for_architecture_v1::<D>("exploration and recovery")?;
            }
            if groups
                .iter()
                .flat_map(|group| group.substeps)
                .any(|substep| {
                    matches!(&substep.forward, NativePolicyForwardInputV1::Packed { .. })
                })
            {
                refuse_for_architecture_v1::<D>("packed forward input")?;
            }
            if self.no_state_digest_columns.is_some() {
                refuse_for_architecture_v1::<D>("no-state-digest install")?;
            }
            if !self.training_math.is_platform_libm() {
                refuse_for_architecture_v1::<D>("portable math")?;
            }
        }
        let setup_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::SetupValidation);
        let math = self.training_math;
        if !math.is_platform_libm() {
            if line_b.is_some() {
                return Err(NativePolicyTrainErrorV1::PortableMathUnsupported { path: "line (b)" });
            }
            if exploration.is_some() {
                return Err(NativePolicyTrainErrorV1::PortableMathUnsupported {
                    path: "recovery loss",
                });
            }
            if imitation {
                return Err(NativePolicyTrainErrorV1::PortableMathUnsupported {
                    path: "imitation",
                });
            }
        }
        // The no-state-digest contract covers the ordinary objective only.
        if self.no_state_digest_columns.is_some() {
            if line_b.is_some() {
                return Err(NativePolicyTrainErrorV1::NoStateDigestUnsupported {
                    path: "auxiliary loss",
                });
            }
            if exploration.is_some() {
                return Err(NativePolicyTrainErrorV1::NoStateDigestUnsupported {
                    path: "recovery loss",
                });
            }
            if imitation {
                return Err(NativePolicyTrainErrorV1::NoStateDigestUnsupported {
                    path: "imitation",
                });
            }
        }
        let mxcsr = self.thread_mxcsr_check_dims_v1();
        mxcsr.verify_v1()?;
        validate_gae_inputs_v1(groups, value_targets, advantages)?;
        if !value_coefficient.is_finite()
            || (value_coefficient <= 0.0 && !(imitation && value_coefficient == 0.0))
        {
            return Err(NativePolicyTrainErrorV1::InvalidValueCoefficient);
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidLearningRate);
        }
        let parameters = self.model.parameter_snapshot_v1();
        validate_parameter_manifest_dims_v1::<D>(&parameters)?;
        validate_optimizer_state_dims_v1::<D>(
            &parameters,
            &self.first_moments,
            &self.second_moments,
        )?;
        validate_canonical_gauge_state(
            &parameters,
            &self.first_moments,
            &self.second_moments,
            self.scorer_bias_anchor_bits,
        )?;
        phase_recorder.finish_v1(setup_timer);
        let forward_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::ForwardLoss);
        let input_major = InputMajorWeightsV1::of_dims_v1::<D>(&parameters);
        let mut precomputed_tapes = precompute_encoded_forward_tapes_dims_v1::<D>(
            &parameters,
            &input_major,
            input_config,
            groups,
            forward_workers,
            math,
            mxcsr,
        );
        let mut selected_outputs = Vec::new();
        let mut physical_terms = Vec::with_capacity(groups.len());
        let mut group_tapes = Vec::with_capacity(groups.len());
        let mut policy_sum = 0.0f32;
        let mut value_sum = 0.0f32;
        for (group_index, group) in groups.iter().enumerate() {
            let substep_count = physical_substep_count_u32_v1(group_index, group.substeps.len())?;
            let mut tapes = Vec::with_capacity(group.substeps.len());
            let mut joint_log_probability = None;
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let NativePolicyForwardInputV1::Encoded(encoded) = &substep.forward else {
                    unreachable!("canonical input checked before forward");
                };
                let recomputed = match precomputed_tapes.as_mut() {
                    Some(tapes) => tapes
                        .next()
                        .expect("one precomputed forward tape per substep")?,
                    None => Box::new(forward_with_tape_input_major_dims_v1::<D>(
                        &parameters,
                        Some(&input_major),
                        input_config,
                        **encoded,
                        math,
                    )?),
                };
                validate_forward_output_bits_v1(
                    &recomputed,
                    substep,
                    group_index,
                    substep_index,
                    ForwardOutputSourceV1::IndependentRecompute,
                )?;
                let tape = DecisionTapeSourceV1::Owned(recomputed);
                if substep.selected_action_index >= tape.logits_v1().len() {
                    return Err(NativePolicyTrainErrorV1::SelectedActionOutOfRange {
                        group_index,
                        substep_index,
                        selected: substep.selected_action_index,
                        action_count: tape.logits_v1().len(),
                    });
                }
                let (selected_log_probability, log_probabilities) = selected_log_softmax_math_v1(
                    tape.logits_v1(),
                    substep.selected_action_index,
                    math,
                )?;
                joint_log_probability = Some(match joint_log_probability {
                    None => selected_log_probability,
                    Some(active) => active + selected_log_probability,
                });
                selected_outputs.push(NativeSelectedOutputV1 {
                    group_index,
                    substep_index,
                    selected_action_index: substep.selected_action_index,
                    selected_logit: tape.logits_v1()[substep.selected_action_index],
                    value: tape.value_v1(),
                    selected_log_probability,
                });
                tapes.push(SelectedDecisionTapeV1 {
                    tape,
                    selected_action_index: substep.selected_action_index,
                    log_probabilities,
                });
            }
            let joint_log_probability = joint_log_probability.expect("nonempty group checked");
            let value = tapes[0].tape.value_v1();
            let advantage = advantages[group_index];
            let value_target = value_targets[group_index];
            let policy_term = -joint_log_probability * advantage;
            let value_error = value - value_target;
            #[cfg(test)]
            record_train_step_v1(|capture| {
                capture.group_roots.push(GroupRootCaptureV1 {
                    advantage,
                    target: value_target,
                    value_error,
                });
            });
            let value_term = value_error * value_error;
            policy_sum += policy_term;
            value_sum += value_term;
            physical_terms.push(NativePhysicalLossTermV1 {
                joint_log_probability,
                value,
                terminal_return: group.terminal_return,
                substep_count,
            });
            group_tapes.push(GroupTapeV1 {
                tapes,
                advantage,
                value_error,
            });
        }
        let group_count = exact_group_count_f32(groups.len())?;
        let mut loss = (policy_sum + value_coefficient * value_sum) / group_count;
        let beta = exploration.map_or(0.0, |e| e.0);
        let regularizer_sum = if exploration.is_some() {
            group_tapes
                .iter()
                .flat_map(|g| &g.tapes)
                .map(|s| exploration_v1::uniform_kl_v1(s.tape.logits_v1()))
                .sum::<f64>()
        } else {
            0.0
        };
        if beta != 0.0 {
            loss += (f64::from(beta) * regularizer_sum / f64::from(group_count)) as f32;
        }
        let diagnostics =
            if exploration.is_some_and(|(_, t, enabled)| enabled && matches!(t, 1 | 32 | 64)) {
                Some(exploration_v1::ExplorationDiagnosticsV1::capture(
                    &parameters,
                    &group_tapes,
                    value_coefficient,
                    group_count,
                    beta,
                )?)
            } else {
                None
            };
        finite_scalar("gae_loss", 0, policy_sum)?;
        finite_scalar("gae_loss", 1, value_sum)?;
        finite_scalar("gae_loss", 2, loss)?;
        // Before the ordinary backward consumes the tapes.
        let line_b_terms = line_b
            .map(|source| match source {
                AuxiliaryTermsSourceV1::InBatch(input) => {
                    line_b_auxiliary_v1::line_b_auxiliary_terms_v1(input, &group_tapes, &parameters)
                }
                AuxiliaryTermsSourceV1::External(input) => {
                    archive_distillation_v1::external_auxiliary_terms_v1(
                        input,
                        &parameters,
                        Some(&input_major),
                        input_config,
                    )
                }
            })
            .transpose()?;

        phase_recorder.finish_v1(forward_timer);
        let backward_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::BackwardGauge);
        // Without the exploration regularizer the sequential backward has an
        // exact parallel form; it runs on the update's forward workers.
        let parallel = match backward_execution {
            BackwardExecutionV1::Sequential if beta == 0.0 => {
                exact_parallel_backward_v1::exact_parallel_backward_dims_v1::<D>(
                    &parameters,
                    &group_tapes,
                    group_count,
                    value_coefficient,
                    forward_workers,
                    math,
                    mxcsr,
                )
            }
            _ => None,
        };
        let (mut gradients, mut gauge_accumulator) = match (parallel, backward_execution) {
            (Some(result), _) => {
                let backward = result?;
                drop(group_tapes);
                backward
            }
            (None, BackwardExecutionV1::Sequential) => {
                let mut gradients = parameters
                    .iter()
                    .map(|parameter| vec![0.0; parameter.values.len()])
                    .collect::<Vec<_>>();
                let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::for_math_v1(math);
                let mut reverse_workspace = ReverseWorkspaceV1::default();
                // Same reverse group/substep/action order as
                // `train_step_with_input_config_v1`'s Sequential arm.
                for group in group_tapes.into_iter().rev() {
                    let d_joint_log_probability = -group.advantage / group_count;
                    let d_value = (value_coefficient / group_count) * (2.0 * group.value_error);
                    for (substep_index, selected) in group.tapes.iter().enumerate().rev() {
                        resize_zeroed_v1(
                            &mut reverse_workspace.grad_output,
                            selected.log_probabilities.len(),
                        );
                        reverse_workspace.grad_output[selected.selected_action_index] =
                            d_joint_log_probability;
                        let grad_output_sum = reverse_workspace
                            .grad_output
                            .iter()
                            .copied()
                            .fold(0.0f32, |sum, value| sum + value);
                        reverse_workspace.d_logits.clear();
                        reverse_workspace
                            .d_logits
                            .reserve(selected.log_probabilities.len());
                        for (gradient, log_probability) in reverse_workspace
                            .grad_output
                            .iter()
                            .zip(&selected.log_probabilities)
                        {
                            reverse_workspace.d_logits.push(
                                *gradient - math.exp_f32_v1(*log_probability) * grad_output_sum,
                            );
                        }
                        if beta != 0.0 {
                            let actor = reverse_workspace.d_logits.clone();
                            let regularizer = exploration_v1::uniform_kl_gradient_v1(
                                &selected.log_probabilities,
                                beta / group_count,
                            );
                            for (d, r) in reverse_workspace.d_logits.iter_mut().zip(&regularizer) {
                                *d += *r;
                            }
                            exploration_v1::observe_regularizer_v1(
                                &mut gauge_accumulator,
                                selected.tape.logits_v1(),
                                &actor,
                                &reverse_workspace.d_logits,
                                beta / group_count,
                            )?;
                        }
                        #[cfg(test)]
                        record_train_step_v1(|capture| {
                            capture.roots.push(LossRootCaptureV1 {
                                substep_index,
                                selected_action_index: selected.selected_action_index,
                                log_probabilities: selected.log_probabilities.clone(),
                                d_joint_log_probability,
                                d_value: if substep_index == 0 { d_value } else { 0.0 },
                                grad_output_sum,
                                d_logits: reverse_workspace.d_logits.clone(),
                            });
                        });
                        gauge_accumulator.observe(
                            selected.tape.logits_v1(),
                            selected.selected_action_index,
                            d_joint_log_probability,
                        )?;
                        reverse_decision_dims_v1::<D>(
                            &parameters,
                            &mut gradients,
                            &selected.tape,
                            &reverse_workspace.d_logits,
                            if substep_index == 0 { d_value } else { 0.0 },
                            &mut reverse_workspace.decision,
                        )?;
                    }
                }
                (gradients, gauge_accumulator)
            }
            (None, BackwardExecutionV1::FixedPartitions { worker_limit }) => {
                let coefficients: Vec<(f32, f32)> = group_tapes
                    .iter()
                    .map(|group| {
                        (
                            -group.advantage / group_count,
                            (value_coefficient / group_count) * (2.0 * group.value_error),
                        )
                    })
                    .collect();
                let gauge_accumulator =
                    observe_scorer_bias_gauge_v1(&group_tapes, &coefficients, math)?;
                let execution_context = FixedPartitionBackwardExecutionContextV1::current_v1();
                let gradients = fixed_partition_backward_gradients_dims_v1::<D>(
                    &parameters,
                    &group_tapes,
                    &coefficients,
                    worker_limit,
                    parameters
                        .iter()
                        .map(|parameter| vec![0.0; parameter.values.len()])
                        .collect::<Vec<_>>(),
                    &execution_context,
                    math,
                    mxcsr,
                )?;
                drop(group_tapes);
                (gradients, gauge_accumulator)
            }
        };
        validate_finite_nested("gae_gradient", &gradients)?;
        // STAGE-B2.md telemetry: the ordinary gradient alone, as Adam would see it.
        let ordinary_only = line_b_terms
            .as_ref()
            .filter(|terms| terms.step_telemetry_v1())
            .map(|_| {
                let mut ordinary = gradients.clone();
                ordinary[SCORER_SECOND_BIAS][0] = 0.0;
                ordinary
            });
        let mut line_b_result = line_b_terms
            .map(|terms| terms.combine_v1(&parameters, &mut gradients, &mut gauge_accumulator))
            .transpose()?;
        validate_finite_nested("gae_line_b_gradient", &gradients)?;
        let raw_scorer_bias_residual = gradients[SCORER_SECOND_BIAS][0];
        let scorer_bias_before_bits = parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        let mut scorer_bias_gauge =
            gauge_accumulator.finish(raw_scorer_bias_residual, scorer_bias_before_bits)?;
        #[cfg(test)]
        record_train_step_v1(|capture| {
            capture.projection.push(ProjectionCaptureV1 {
                gradients: gradients.clone(),
                residual_bits: scorer_bias_gauge.raw_gradient_residual.to_bits(),
            });
        });
        gradients[SCORER_SECOND_BIAS][0] = 0.0;

        phase_recorder.finish_v1(backward_timer);
        let adam_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::AdamMath);
        let next_step = self
            .adam_step
            .checked_add(1)
            .ok_or(NativePolicyTrainErrorV1::AdamStepOverflow)?;
        let (mut next_parameters, mut next_first_moments, mut next_second_moments) = adam_update(
            &parameters,
            &gradients,
            &self.first_moments,
            &self.second_moments,
            next_step,
            learning_rate,
            math,
        )?;
        next_parameters[SCORER_SECOND_BIAS].values[0] =
            f32::from_bits(self.scorer_bias_anchor_bits);
        next_first_moments[SCORER_SECOND_BIAS][0] = 0.0;
        next_second_moments[SCORER_SECOND_BIAS][0] = 0.0;
        scorer_bias_gauge.parameter_after_bits =
            next_parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        if let (Some(ordinary), Some(result)) = (&ordinary_only, line_b_result.as_mut()) {
            let (mut alone, _, _) = adam_update(
                &parameters,
                ordinary,
                &self.first_moments,
                &self.second_moments,
                next_step,
                learning_rate,
                math,
            )?;
            alone[SCORER_SECOND_BIAS].values[0] = f32::from_bits(self.scorer_bias_anchor_bits);
            let (mut combined, mut ordinary_step, mut difference) = (0.0_f64, 0.0_f64, 0.0_f64);
            for ((before, after), only) in parameters.iter().zip(&next_parameters).zip(&alone) {
                for ((&x, &y), &o) in before.values.iter().zip(&after.values).zip(&only.values) {
                    let (x, y, o) = (f64::from(x), f64::from(y), f64::from(o));
                    combined += (y - x) * (y - x);
                    ordinary_step += (o - x) * (o - x);
                    difference += (y - o) * (y - o);
                }
            }
            result.adam_steps = Some(serde_json::json!({"combined_l2":combined.sqrt(),
                "ordinary_l2":ordinary_step.sqrt(),"difference_l2":difference.sqrt()}));
        }

        phase_recorder.finish_v1(adam_timer);
        let finalization_timer =
            phase_recorder.start_v1(NativeTrainingPhaseV1::FinalizationCloning);
        let mut candidate_model = self.model.clone();
        candidate_model.replace_parameter_snapshot_v1(&next_parameters)?;
        validate_optimizer_state_dims_v1::<D>(
            &next_parameters,
            &next_first_moments,
            &next_second_moments,
        )?;
        validate_canonical_gauge_state(
            &next_parameters,
            &next_first_moments,
            &next_second_moments,
            self.scorer_bias_anchor_bits,
        )?;
        self.check_no_state_digest_candidate_dims_v1(
            &next_parameters,
            &next_first_moments,
            &next_second_moments,
        )?;

        let exploration_result = exploration.map(|(beta, update, _)| serde_json::json!({
            "schema":"native-uniform-kl-recovery/v1", "update_index":update,"beta_bits":beta.to_bits(),
            "regularizer_sum":regularizer_sum,"normalization_physical_groups":group_count,
            "objective":"gae_uniform_kl_recovery/v1", "diagnostics":diagnostics.map(|d|d.finish(&parameters,&gradients,&next_parameters))
        }));
        let gradient_snapshot = named_state_snapshot(&parameters, &gradients);
        self.model = candidate_model;
        self.adam_step = next_step;
        self.first_moments = next_first_moments;
        self.second_moments = next_second_moments;
        phase_recorder.finish_v1(finalization_timer);
        Ok((
            NativePolicyTrainStepResultV1 {
                policy_sum,
                value_sum,
                loss,
                adam_step: next_step,
                selected_outputs,
                physical_terms,
                gradients: gradient_snapshot,
                scorer_bias_gauge,
            },
            line_b_result,
            exploration_result,
        ))
    }
}

/// Fewest substeps a forward worker is given before another thread is worth
/// spawning; one forward pass costs tens of microseconds.
const GAE_FORWARD_MIN_SUBSTEPS_PER_WORKER_V1: usize = 32;

enum GaeForwardOutcomeV1 {
    Completed(Result<Box<DecisionTapeV1>, NativePolicyTrainErrorV1>),
    Panicked(Box<dyn std::any::Any + Send + 'static>),
}

/// Every substep's independent forward tape, in group/substep order, computed
/// on up to `forward_workers` scoped threads. `None` (one worker, or too few
/// substeps to share) keeps the inline forward in the caller's loop. A tape is
/// a pure function of the parameter snapshot and its encoded input, so the
/// thread that computes it changes no bit. The caller consumes the outcomes
/// strictly in order and a worker's error or panic surfaces only when the loop
/// reaches that substep, so the first failure reported is the one the inline
/// loop would have hit.
fn precompute_encoded_forward_tapes_v1(
    parameters: &[NativeNamedParameterV1],
    input_major: &InputMajorWeightsV1,
    config: NativePolicyValueModelConfigV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    forward_workers: usize,
    math: TrainingMathV1,
    mxcsr: ThreadMxcsrCheckV1,
) -> Option<impl Iterator<Item = Result<Box<DecisionTapeV1>, NativePolicyTrainErrorV1>>> {
    precompute_encoded_forward_tapes_dims_v1::<Net8DimsV1>(
        parameters,
        input_major,
        config,
        groups,
        forward_workers,
        math,
        mxcsr,
    )
}

/// [`precompute_encoded_forward_tapes_v1`] at the dims `D`.
fn precompute_encoded_forward_tapes_dims_v1<D: NetDimsV1>(
    parameters: &[NativeNamedParameterV1],
    input_major: &InputMajorWeightsV1,
    config: NativePolicyValueModelConfigV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    forward_workers: usize,
    math: TrainingMathV1,
    mxcsr: ThreadMxcsrCheckV1,
) -> Option<impl Iterator<Item = Result<Box<DecisionTapeV1>, NativePolicyTrainErrorV1>>> {
    let mut tasks = Vec::new();
    for group in groups {
        for substep in group.substeps.iter() {
            let NativePolicyForwardInputV1::Encoded(encoded) = &substep.forward else {
                unreachable!("canonical input checked before forward");
            };
            tasks.push(**encoded);
        }
    }
    let worker_count = forward_workers.min(tasks.len() / GAE_FORWARD_MIN_SUBSTEPS_PER_WORKER_V1);
    #[cfg(test)]
    record_effective_forward_workers_for_test_v1(worker_count.max(1));
    if worker_count < 2 {
        return None;
    }
    let chunk_size = tasks.len().div_ceil(worker_count);
    let outcomes = thread::scope(|scope| {
        let jobs = tasks
            .chunks(chunk_size)
            .enumerate()
            .map(|(worker_index, chunk)| {
                let spawned = thread::Builder::new()
                    .name(format!("native-gae-forward-{worker_index}"))
                    .spawn_scoped(scope, move || {
                        // A worker outside the pinned MXCSR state fails each
                        // of its substeps without computing them.
                        if let Err(error) = mxcsr.verify_worker_v1() {
                            return chunk
                                .iter()
                                .map(|_| GaeForwardOutcomeV1::Completed(Err(error.clone())))
                                .collect();
                        }
                        forward_encoded_chunk_dims_v1::<D>(
                            parameters,
                            input_major,
                            config,
                            chunk,
                            math,
                        )
                    });
                (spawned, chunk)
            })
            .collect::<Vec<_>>();
        let mut outcomes = Vec::with_capacity(tasks.len());
        for (spawned, chunk) in jobs {
            match spawned {
                Ok(worker) => match worker.join() {
                    Ok(mut worker_outcomes) => {
                        // The test-only forward counter is per thread; credit
                        // the caller with the calls made on its behalf.
                        #[cfg(test)]
                        FORWARD_WITH_TAPE_CALL_COUNT_V1
                            .with(|count| count.set(count.get() + chunk.len() as u64));
                        outcomes.append(&mut worker_outcomes);
                    }
                    Err(payload) => resume_unwind(payload),
                },
                Err(_) => outcomes.extend(forward_encoded_chunk_dims_v1::<D>(
                    parameters,
                    input_major,
                    config,
                    chunk,
                    math,
                )),
            }
        }
        outcomes
    });
    Some(outcomes.into_iter().map(|outcome| match outcome {
        GaeForwardOutcomeV1::Completed(result) => result,
        GaeForwardOutcomeV1::Panicked(payload) => resume_unwind(payload),
    }))
}

fn forward_encoded_chunk_v1(
    parameters: &[NativeNamedParameterV1],
    input_major: &InputMajorWeightsV1,
    config: NativePolicyValueModelConfigV1,
    chunk: &[NativeEncodedDecisionViewV1<'_>],
    math: TrainingMathV1,
) -> Vec<GaeForwardOutcomeV1> {
    forward_encoded_chunk_dims_v1::<Net8DimsV1>(parameters, input_major, config, chunk, math)
}

fn forward_encoded_chunk_dims_v1<D: NetDimsV1>(
    parameters: &[NativeNamedParameterV1],
    input_major: &InputMajorWeightsV1,
    config: NativePolicyValueModelConfigV1,
    chunk: &[NativeEncodedDecisionViewV1<'_>],
    math: TrainingMathV1,
) -> Vec<GaeForwardOutcomeV1> {
    chunk
        .iter()
        .map(|encoded| {
            match catch_unwind(AssertUnwindSafe(|| {
                forward_with_tape_input_major_dims_v1::<D>(
                    parameters,
                    Some(input_major),
                    config,
                    *encoded,
                    math,
                )
                .map(Box::new)
            })) {
                Ok(result) => GaeForwardOutcomeV1::Completed(result),
                Err(payload) => GaeForwardOutcomeV1::Panicked(payload),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod wide_refusal_tests;
