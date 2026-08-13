use crate::{
    begin_player_visible_attacker_deliberation_v1,
    begin_player_visible_single_attacker_blocker_deliberation_v1,
    parse_and_validate_visible_duel_producer_result_v1,
    prepare_player_visible_attacker_execution_plan_v1,
    prepare_player_visible_multi_attacker_blocker_execution_step_v1,
    prepare_player_visible_single_attacker_blocker_execution_plan_v1,
    reobserve_player_visible_attacker_execution_step_v1,
    reobserve_player_visible_single_attacker_blocker_execution_step_v1,
    score_and_advance_player_visible_attacker_deliberation_v1,
    score_and_advance_player_visible_single_attacker_blocker_deliberation_v1,
    score_and_select_player_visible_blocker_target_v1,
    score_and_select_player_visible_multi_attacker_blocker_v1,
    CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1, MtgoContractErrorV1,
    MtgoPlayerVisibleAttackerDeliberationProgressV1, MtgoPlayerVisibleAttackerExecutionOperationV1,
    MtgoPlayerVisibleAttackerExecutionPlanV1, MtgoPlayerVisibleAttackerScorerV1,
    MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1,
    MtgoPlayerVisibleMultiAttackerBlockerScorerV1, MtgoPlayerVisibleObjectRefV1,
    MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1,
    MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1,
    MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1,
    MtgoPlayerVisibleSingleAttackerBlockerScorerV1,
    MtgoVisibleDuelViewModelBrokerAbstentionReasonV1, MtgoVisibleDuelViewModelBrokerResultV1,
};
use sha2::{Digest, Sha256};

const COMBAT_SCORING_BRIDGE_DOMAIN_V1: &[u8] = b"mtgo-player-visible-combat-scoring-bridge-v1";
const COMBAT_EXECUTION_STEP_DOMAIN_V1: &[u8] = b"mtgo-player-visible-combat-execution-step-v1";
const COMBAT_VISIBLE_TRANSITION_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-combat-visible-transition-v1";

/// One model implementation for each player-visible combat decision shape.
/// Every inherited method receives a type that cannot represent raw MTGO
/// objects, client identifiers, hidden zones, process data, or transport
/// metadata.
pub trait MtgoPlayerVisibleCombatScorerV1:
    MtgoPlayerVisibleAttackerScorerV1
    + MtgoPlayerVisibleSingleAttackerBlockerScorerV1
    + MtgoPlayerVisibleMultiAttackerBlockerScorerV1
{
}

impl<T> MtgoPlayerVisibleCombatScorerV1 for T where
    T: MtgoPlayerVisibleAttackerScorerV1
        + MtgoPlayerVisibleSingleAttackerBlockerScorerV1
        + MtgoPlayerVisibleMultiAttackerBlockerScorerV1
{
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPlayerVisiblePreparedCombatKindV1 {
    AttackerPlan,
    SingleAttackerBlockerPlan,
    MultiAttackerBlockerStep,
}

enum MtgoPlayerVisiblePreparedCombatExecutionV1 {
    Attacker(MtgoPlayerVisibleAttackerExecutionPlanV1),
    SingleAttackerBlocker(MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1),
    MultiAttackerBlocker(CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1),
}

/// A model-owned combat choice connected to a coordinate-free execution
/// contract and bound to the exact sanitized producer bytes that the model
/// evaluated. This is checked-untrusted and intentionally has no conversion to
/// a process, command, input primitive, event entry, or spending operation.
///
/// The model-selection commitments cover each sequential inclusion decision,
/// so the final attacker or single-attacker blocker mask cannot be detached
/// from the model calls that produced it.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1;
/// fn cannot_dispatch(value: &CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1) {
///     let _ = value.dispatch();
///     let _ = value.client_object();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1 {
    kind: MtgoPlayerVisiblePreparedCombatKindV1,
    exact_producer_result_sha256: String,
    deployment_commitment_sha256: String,
    model_selection_commitments_sha256: Vec<String>,
    execution_commitment_sha256: String,
    bridge_commitment_sha256: String,
    execution: MtgoPlayerVisiblePreparedCombatExecutionV1,
}

impl CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1 {
    pub fn kind_v1(&self) -> MtgoPlayerVisiblePreparedCombatKindV1 {
        self.kind
    }

    pub fn exact_producer_result_sha256_v1(&self) -> &str {
        &self.exact_producer_result_sha256
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.deployment_commitment_sha256
    }

    pub fn model_selection_count_v1(&self) -> usize {
        self.model_selection_commitments_sha256.len()
    }

    pub fn execution_commitment_sha256_v1(&self) -> &str {
        &self.execution_commitment_sha256
    }

    pub fn bridge_commitment_sha256_v1(&self) -> &str {
        &self.bridge_commitment_sha256
    }

    pub fn attacker_execution_plan_v1(&self) -> Option<&MtgoPlayerVisibleAttackerExecutionPlanV1> {
        match &self.execution {
            MtgoPlayerVisiblePreparedCombatExecutionV1::Attacker(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn single_attacker_blocker_execution_plan_v1(
        &self,
    ) -> Option<&MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1> {
        match &self.execution {
            MtgoPlayerVisiblePreparedCombatExecutionV1::SingleAttackerBlocker(plan) => Some(plan),
            _ => None,
        }
    }

    pub fn multi_attacker_blocker_execution_step_v1(
        &self,
    ) -> Option<&CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1> {
        match &self.execution {
            MtgoPlayerVisiblePreparedCombatExecutionV1::MultiAttackerBlocker(step) => Some(step),
            _ => None,
        }
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

pub enum CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1 {
    Abstained {
        reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1,
    },
    Prepared(CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1),
}

/// Exact coordinate-free arguments accepted by the sealed combat producer.
/// Every field is either a commitment or a fact already present in the
/// sanitized player-visible result. No client object, hidden target, process,
/// coordinate, input method, event-entry operation, or spending operation can
/// be represented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtgoPlayerVisibleCombatBrokerCommandV1 {
    AttackerPlan {
        current_selection_sha256: String,
        candidate_count: usize,
        desired_mask_hex: String,
        plan_commitment_sha256: String,
    },
    SingleAttackerBlockerPlan {
        current_selection_sha256: String,
        candidate_count: usize,
        desired_mask_hex: String,
        plan_commitment_sha256: String,
    },
    MultiAttackerBlockerStep {
        current_selection_sha256: String,
        selected_index: usize,
        operation_kind: char,
        blocker_visible_ordinal: Option<u32>,
        attacker_visible_ordinal: Option<u32>,
        model_selection_commitment_sha256: String,
        execution_step_commitment_sha256: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPlayerVisibleCombatSubmittedOperationV1 {
    ToggleAttacker {
        attacker: MtgoPlayerVisibleObjectRefV1,
        select_for_attack: bool,
    },
    FinishAttackers,
    AddSingleAttackerBlocker {
        blocker: MtgoPlayerVisibleObjectRefV1,
    },
    FinishSingleAttackerBlockers,
    FinishMultiAttackerBlockers,
    ChooseMultiAttackerBlocker {
        blocker: MtgoPlayerVisibleObjectRefV1,
    },
    ChooseAttackerForBlocker {
        blocker: MtgoPlayerVisibleObjectRefV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPlayerVisibleCombatTransitionProgressV1 {
    ContinueSamePlan,
    AwaitFreshCombatModelDecision,
    CombatDeclarationComplete,
}

/// One broker-ready combat operation derived from a fresh complete sanitized
/// result. The prepared model choice remains private and move-only so an
/// operation cannot be confirmed against a different plan.
pub struct CheckedUntrustedMtgoPlayerVisibleCombatExecutionStepV1 {
    prepared: CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1,
    current: MtgoVisibleDuelViewModelBrokerResultV1,
    command: MtgoPlayerVisibleCombatBrokerCommandV1,
    operation: MtgoPlayerVisibleCombatSubmittedOperationV1,
    current_selection_sha256: String,
    execution_step_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleCombatExecutionStepV1 {
    pub fn command_v1(&self) -> &MtgoPlayerVisibleCombatBrokerCommandV1 {
        &self.command
    }

    pub fn operation_v1(&self) -> MtgoPlayerVisibleCombatSubmittedOperationV1 {
        self.operation
    }

    pub fn current_selection_sha256_v1(&self) -> &str {
        &self.current_selection_sha256
    }

    pub fn execution_step_commitment_sha256_v1(&self) -> &str {
        &self.execution_step_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// A submitted step whose exact intended public transition was found in a
/// later complete sanitized producer result. The only continuation can return
/// the same retained plan for another freshly observed monotonic step.
pub struct CheckedUntrustedMtgoPlayerVisibleCombatTransitionV1 {
    prepared: Option<CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1>,
    progress: MtgoPlayerVisibleCombatTransitionProgressV1,
    operation: MtgoPlayerVisibleCombatSubmittedOperationV1,
    before_selection_sha256: String,
    after_selection_sha256: String,
    confirmation_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleCombatTransitionV1 {
    pub fn progress_v1(&self) -> MtgoPlayerVisibleCombatTransitionProgressV1 {
        self.progress
    }

    pub fn operation_v1(&self) -> MtgoPlayerVisibleCombatSubmittedOperationV1 {
        self.operation
    }

    pub fn before_selection_sha256_v1(&self) -> &str {
        &self.before_selection_sha256
    }

    pub fn after_selection_sha256_v1(&self) -> &str {
        &self.after_selection_sha256
    }

    pub fn confirmation_commitment_sha256_v1(&self) -> &str {
        &self.confirmation_commitment_sha256
    }

    pub fn into_prepared_continuation_v1(
        mut self,
    ) -> Result<CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1, MtgoContractErrorV1> {
        if self.progress != MtgoPlayerVisibleCombatTransitionProgressV1::ContinueSamePlan {
            return Err(error_v1(
                "visible_combat_transition_continuation",
                "only a confirmed intermediate attacker or single-blocker step may continue the same plan",
            ));
        }
        self.prepared.take().ok_or_else(|| {
            error_v1(
                "visible_combat_transition_continuation",
                "the confirmed transition no longer owns its prepared plan",
            )
        })
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Reconciles a retained model-prepared combat choice against the exact latest
/// sanitized player-visible result and produces exactly one sealed broker
/// operation. Attacker and single-blocker plans may require several calls, but
/// every call must use a newly observed result after the prior transition.
pub fn prepare_player_visible_combat_execution_step_v1(
    prepared: CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1,
    exact_current_producer_result: &[u8],
) -> Result<CheckedUntrustedMtgoPlayerVisibleCombatExecutionStepV1, MtgoContractErrorV1> {
    let current =
        parse_and_validate_visible_duel_producer_result_v1(exact_current_producer_result)?;
    let current_selection_sha256 = sha256_v1(exact_current_producer_result);
    let (command, operation) = match &prepared.execution {
        MtgoPlayerVisiblePreparedCombatExecutionV1::Attacker(plan) => {
            let step = reobserve_player_visible_attacker_execution_step_v1(
                plan,
                exact_current_producer_result,
            )?;
            let operation = match step.operation_v1() {
                MtgoPlayerVisibleAttackerExecutionOperationV1::Toggle {
                    attacker,
                    select_for_attack,
                } => MtgoPlayerVisibleCombatSubmittedOperationV1::ToggleAttacker {
                    attacker,
                    select_for_attack,
                },
                MtgoPlayerVisibleAttackerExecutionOperationV1::Done => {
                    MtgoPlayerVisibleCombatSubmittedOperationV1::FinishAttackers
                }
            };
            (
                MtgoPlayerVisibleCombatBrokerCommandV1::AttackerPlan {
                    current_selection_sha256: step.current_selection_sha256_v1().to_owned(),
                    candidate_count: plan.candidate_count_v1(),
                    desired_mask_hex: plan.desired_attacker_mask_hex_v1().to_owned(),
                    plan_commitment_sha256: step.plan_commitment_sha256_v1().to_owned(),
                },
                operation,
            )
        }
        MtgoPlayerVisiblePreparedCombatExecutionV1::SingleAttackerBlocker(plan) => {
            let step = reobserve_player_visible_single_attacker_blocker_execution_step_v1(
                plan,
                exact_current_producer_result,
            )?;
            let operation = match step.operation_v1() {
                MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1::AddBlocker {
                    blocker,
                } => MtgoPlayerVisibleCombatSubmittedOperationV1::AddSingleAttackerBlocker {
                    blocker,
                },
                MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1::Done => {
                    MtgoPlayerVisibleCombatSubmittedOperationV1::FinishSingleAttackerBlockers
                }
            };
            (
                MtgoPlayerVisibleCombatBrokerCommandV1::SingleAttackerBlockerPlan {
                    current_selection_sha256: step.current_selection_sha256_v1().to_owned(),
                    candidate_count: plan.candidate_count_v1(),
                    desired_mask_hex: plan.desired_blocker_mask_hex_v1().to_owned(),
                    plan_commitment_sha256: step.plan_commitment_sha256_v1().to_owned(),
                },
                operation,
            )
        }
        MtgoPlayerVisiblePreparedCombatExecutionV1::MultiAttackerBlocker(step) => {
            if step.source_selection_sha256_v1() != current_selection_sha256 {
                return Err(error_v1(
                    "visible_combat_execution_multi_source",
                    "a one-step multi-attacker blocker choice must use the exact sanitized result scored by the model",
                ));
            }
            let (operation, operation_kind, blocker, attacker) = match step.operation_v1() {
                MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::FinishBlocking => (
                    MtgoPlayerVisibleCombatSubmittedOperationV1::FinishMultiAttackerBlockers,
                    'f',
                    None,
                    None,
                ),
                MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseBlocker {
                    blocker,
                } => (
                    MtgoPlayerVisibleCombatSubmittedOperationV1::ChooseMultiAttackerBlocker {
                        blocker,
                    },
                    'b',
                    Some(blocker.visible_ordinal),
                    None,
                ),
                MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseAttackerForBlocker {
                    blocker,
                    attacker,
                } => (
                    MtgoPlayerVisibleCombatSubmittedOperationV1::ChooseAttackerForBlocker {
                        blocker,
                        attacker,
                    },
                    't',
                    Some(blocker.visible_ordinal),
                    Some(attacker.visible_ordinal),
                ),
            };
            (
                MtgoPlayerVisibleCombatBrokerCommandV1::MultiAttackerBlockerStep {
                    current_selection_sha256: step.source_selection_sha256_v1().to_owned(),
                    selected_index: step.selected_index_v1(),
                    operation_kind,
                    blocker_visible_ordinal: blocker,
                    attacker_visible_ordinal: attacker,
                    model_selection_commitment_sha256: step
                        .model_selection_commitment_sha256_v1()
                        .to_owned(),
                    execution_step_commitment_sha256: step
                        .execution_step_commitment_sha256_v1()
                        .to_owned(),
                },
                operation,
            )
        }
    };
    if command_current_selection_sha256_v1(&command) != current_selection_sha256 {
        return Err(error_v1(
            "visible_combat_execution_current_source",
            "the sealed command must retain the exact current sanitized result hash",
        ));
    }
    let command_commitment_sha256 = command_commitment_sha256_v1(&command);
    let operation_commitment_sha256 = operation_commitment_sha256_v1(operation);
    let execution_step_commitment_sha256 = commitment_parts_v1(
        COMBAT_EXECUTION_STEP_DOMAIN_V1,
        &[
            prepared.bridge_commitment_sha256.as_bytes(),
            current_selection_sha256.as_bytes(),
            command_commitment_sha256.as_bytes(),
            operation_commitment_sha256.as_bytes(),
            b"one_sealed_step_pending_fresh_visible_transition",
        ],
    );
    Ok(CheckedUntrustedMtgoPlayerVisibleCombatExecutionStepV1 {
        prepared,
        current,
        command,
        operation,
        current_selection_sha256,
        execution_step_commitment_sha256,
    })
}

fn command_current_selection_sha256_v1(command: &MtgoPlayerVisibleCombatBrokerCommandV1) -> &str {
    match command {
        MtgoPlayerVisibleCombatBrokerCommandV1::AttackerPlan {
            current_selection_sha256,
            ..
        }
        | MtgoPlayerVisibleCombatBrokerCommandV1::SingleAttackerBlockerPlan {
            current_selection_sha256,
            ..
        }
        | MtgoPlayerVisibleCombatBrokerCommandV1::MultiAttackerBlockerStep {
            current_selection_sha256,
            ..
        } => current_selection_sha256,
    }
}

fn command_commitment_sha256_v1(command: &MtgoPlayerVisibleCombatBrokerCommandV1) -> String {
    match command {
        MtgoPlayerVisibleCombatBrokerCommandV1::AttackerPlan {
            current_selection_sha256,
            candidate_count,
            desired_mask_hex,
            plan_commitment_sha256,
        } => commitment_parts_v1(
            b"mtgo-player-visible-combat-broker-attacker-command-v1",
            &[
                current_selection_sha256.as_bytes(),
                candidate_count.to_string().as_bytes(),
                desired_mask_hex.as_bytes(),
                plan_commitment_sha256.as_bytes(),
            ],
        ),
        MtgoPlayerVisibleCombatBrokerCommandV1::SingleAttackerBlockerPlan {
            current_selection_sha256,
            candidate_count,
            desired_mask_hex,
            plan_commitment_sha256,
        } => commitment_parts_v1(
            b"mtgo-player-visible-combat-broker-single-blocker-command-v1",
            &[
                current_selection_sha256.as_bytes(),
                candidate_count.to_string().as_bytes(),
                desired_mask_hex.as_bytes(),
                plan_commitment_sha256.as_bytes(),
            ],
        ),
        MtgoPlayerVisibleCombatBrokerCommandV1::MultiAttackerBlockerStep {
            current_selection_sha256,
            selected_index,
            operation_kind,
            blocker_visible_ordinal,
            attacker_visible_ordinal,
            model_selection_commitment_sha256,
            execution_step_commitment_sha256,
        } => commitment_parts_v1(
            b"mtgo-player-visible-combat-broker-multi-blocker-command-v1",
            &[
                current_selection_sha256.as_bytes(),
                selected_index.to_string().as_bytes(),
                operation_kind.to_string().as_bytes(),
                blocker_visible_ordinal
                    .map_or_else(|| "-".to_owned(), |value| value.to_string())
                    .as_bytes(),
                attacker_visible_ordinal
                    .map_or_else(|| "-".to_owned(), |value| value.to_string())
                    .as_bytes(),
                model_selection_commitment_sha256.as_bytes(),
                execution_step_commitment_sha256.as_bytes(),
            ],
        ),
    }
}

fn validate_combat_visible_transition_v1(
    prepared: &CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1,
    before: &MtgoVisibleDuelViewModelBrokerResultV1,
    after: &MtgoVisibleDuelViewModelBrokerResultV1,
    operation: MtgoPlayerVisibleCombatSubmittedOperationV1,
) -> Result<MtgoPlayerVisibleCombatTransitionProgressV1, MtgoContractErrorV1> {
    match operation {
        MtgoPlayerVisibleCombatSubmittedOperationV1::ToggleAttacker {
            attacker,
            select_for_attack,
        } => {
            let before = exact_attacker_selection_result_v1(before)?;
            let after = exact_attacker_selection_result_v1(after)?;
            validate_same_attacker_transition_universe_v1(before, after)?;
            let mut changed = 0usize;
            for (left, right) in before
                .ordered_candidates
                .iter()
                .zip(&after.ordered_candidates)
            {
                if left.attacker == attacker {
                    if left.currently_attacking == select_for_attack
                        || right.currently_attacking != select_for_attack
                    {
                        return Err(error_v1(
                            "visible_combat_transition_attacker",
                            "the selected attacker did not visibly toggle to the intended state",
                        ));
                    }
                    changed += 1;
                } else if left != right {
                    return Err(error_v1(
                        "visible_combat_transition_attacker_extra",
                        "an unrelated visible attacker candidate changed during one toggle",
                    ));
                }
            }
            if changed != 1 {
                return Err(error_v1(
                    "visible_combat_transition_attacker_identity",
                    "the toggled attacker must occur exactly once in the visible candidate order",
                ));
            }
            validate_attacker_selection_consistency_v1(after)?;
            Ok(MtgoPlayerVisibleCombatTransitionProgressV1::ContinueSamePlan)
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::FinishAttackers => {
            let before = exact_attacker_selection_result_v1(before)?;
            require_attacker_plan_reconciled_v1(prepared, before)?;
            let after_state = result_visible_state_v1(after)?;
            let expected = before
                .ordered_candidates
                .iter()
                .filter(|candidate| candidate.currently_attacking)
                .map(|candidate| candidate.attacker)
                .collect::<Vec<_>>();
            if !after_state.combat.attackers_declared
                || after_state.combat.ordered_attackers != expected
            {
                return Err(error_v1(
                    "visible_combat_transition_attackers_done",
                    "attacker completion requires the exact selected attackers to be visibly declared",
                ));
            }
            Ok(MtgoPlayerVisibleCombatTransitionProgressV1::CombatDeclarationComplete)
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::AddSingleAttackerBlocker { blocker } => {
            let before = exact_single_blocker_state_result_v1(before)?;
            let after = exact_single_blocker_state_result_v1(after)?;
            validate_same_single_blocker_transition_universe_v1(before, after)?;
            let mut changed = 0usize;
            for (left, right) in before
                .ordered_candidates
                .iter()
                .zip(&after.ordered_candidates)
            {
                if left.blocker == blocker {
                    if left.currently_blocking || !right.currently_blocking {
                        return Err(error_v1(
                            "visible_combat_transition_single_blocker",
                            "the selected blocker did not visibly enter the intended assignment",
                        ));
                    }
                    changed += 1;
                } else if left != right {
                    return Err(error_v1(
                        "visible_combat_transition_single_blocker_extra",
                        "an unrelated visible blocker candidate changed during one assignment",
                    ));
                }
            }
            if changed != 1
                || !state_has_exact_blocker_assignment_v1(
                    &after.current_state,
                    after.attacker,
                    blocker,
                )
            {
                return Err(error_v1(
                    "visible_combat_transition_single_blocker_identity",
                    "the added blocker must occur exactly once under the exact visible attacker",
                ));
            }
            let before_pairs = assignment_pairs_v1(&before.current_state);
            let after_pairs = assignment_pairs_v1(&after.current_state);
            if after_pairs.len() != before_pairs.len().saturating_add(1)
                || !before_pairs.iter().all(|pair| after_pairs.contains(pair))
                || !after_pairs.contains(&(after.attacker, blocker))
            {
                return Err(error_v1(
                    "visible_combat_transition_single_blocker_extra",
                    "one single-attacker blocker step must add exactly one visible assignment",
                ));
            }
            Ok(MtgoPlayerVisibleCombatTransitionProgressV1::ContinueSamePlan)
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::FinishSingleAttackerBlockers => {
            let before = exact_single_blocker_state_result_v1(before)?;
            require_single_blocker_plan_reconciled_v1(prepared, before)?;
            let after_state = result_visible_state_v1(after)?;
            if !after_state.combat.blockers_declared
                || after_state.combat.blocker_assignments
                    != before.current_state.combat.blocker_assignments
            {
                return Err(error_v1(
                    "visible_combat_transition_single_blockers_done",
                    "single-attacker blocker completion requires the exact rendered assignments to be declared",
                ));
            }
            Ok(MtgoPlayerVisibleCombatTransitionProgressV1::CombatDeclarationComplete)
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::ChooseMultiAttackerBlocker { blocker } => {
            let before = exact_multi_blocker_selection_result_v1(before)?;
            let after = exact_blocker_target_selection_result_v1(after)?;
            if !before.ordered_available_blockers.contains(&blocker)
                || after.blocker != blocker
                || after.ordered_visible_targetable_attackers.is_empty()
                || before.current_state != after.current_state
            {
                return Err(error_v1(
                    "visible_combat_transition_blocker_prompt",
                    "choosing a blocker must visibly open its exact nonempty attacker target prompt without changing public state",
                ));
            }
            Ok(MtgoPlayerVisibleCombatTransitionProgressV1::AwaitFreshCombatModelDecision)
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::ChooseAttackerForBlocker {
            blocker,
            attacker,
        } => {
            let before = exact_blocker_target_selection_result_v1(before)?;
            let after = exact_multi_blocker_selection_result_v1(after)?;
            if before.blocker != blocker
                || !before
                    .ordered_visible_targetable_attackers
                    .contains(&attacker)
                || !state_has_exact_blocker_assignment_v1(&after.current_state, attacker, blocker)
            {
                return Err(error_v1(
                    "visible_combat_transition_blocker_assignment",
                    "choosing an attacker must visibly assign the exact blocker under that attacker",
                ));
            }
            let mut before_state = before.current_state.clone();
            let mut after_state = after.current_state.clone();
            before_state.combat.blocker_assignments.clear();
            after_state.combat.blocker_assignments.clear();
            if before_state != after_state {
                return Err(error_v1(
                    "visible_combat_transition_blocker_state",
                    "only the rendered blocker assignment may change after target selection",
                ));
            }
            let before_pairs = assignment_pairs_v1(&before.current_state);
            let after_pairs = assignment_pairs_v1(&after.current_state);
            if after_pairs.len() != before_pairs.len().saturating_add(1)
                || !before_pairs.iter().all(|pair| after_pairs.contains(pair))
            {
                return Err(error_v1(
                    "visible_combat_transition_blocker_extra",
                    "target selection must add exactly one visible blocker assignment",
                ));
            }
            Ok(MtgoPlayerVisibleCombatTransitionProgressV1::AwaitFreshCombatModelDecision)
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::FinishMultiAttackerBlockers => {
            let before = exact_multi_blocker_selection_result_v1(before)?;
            let after_state = result_visible_state_v1(after)?;
            if !after_state.combat.blockers_declared
                || after_state.combat.blocker_assignments
                    != before.current_state.combat.blocker_assignments
            {
                return Err(error_v1(
                    "visible_combat_transition_multi_blockers_done",
                    "multi-attacker blocker completion requires the exact rendered assignments to be declared",
                ));
            }
            Ok(MtgoPlayerVisibleCombatTransitionProgressV1::CombatDeclarationComplete)
        }
    }
}

fn exact_attacker_selection_result_v1(
    result: &MtgoVisibleDuelViewModelBrokerResultV1,
) -> Result<&crate::MtgoPlayerVisibleAttackerSelectionInputV1, MtgoContractErrorV1> {
    match result {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { selection } => {
            Ok(selection)
        }
        _ => Err(error_v1(
            "visible_combat_transition_attacker_kind",
            "an attacker toggle requires a fresh complete attacker-selection result",
        )),
    }
}

fn exact_single_blocker_state_result_v1(
    result: &MtgoVisibleDuelViewModelBrokerResultV1,
) -> Result<&crate::MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1, MtgoContractErrorV1> {
    match result {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
            selection,
        }
        | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
            selection,
        } => Ok(selection),
        _ => Err(error_v1(
            "visible_combat_transition_single_blocker_kind",
            "a single-attacker blocker step requires a fresh complete visible blocker state",
        )),
    }
}

fn exact_multi_blocker_selection_result_v1(
    result: &MtgoVisibleDuelViewModelBrokerResultV1,
) -> Result<&crate::MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1, MtgoContractErrorV1> {
    match result {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
            selection,
        } => Ok(selection),
        _ => Err(error_v1(
            "visible_combat_transition_multi_blocker_kind",
            "this combat transition requires a fresh complete multi-attacker blocker result",
        )),
    }
}

fn exact_blocker_target_selection_result_v1(
    result: &MtgoVisibleDuelViewModelBrokerResultV1,
) -> Result<&crate::MtgoPlayerVisibleBlockerTargetSelectionInputV1, MtgoContractErrorV1> {
    match result {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { selection } => {
            Ok(selection)
        }
        _ => Err(error_v1(
            "visible_combat_transition_blocker_target_kind",
            "this combat transition requires the exact visible blocker target prompt",
        )),
    }
}

fn result_visible_state_v1(
    result: &MtgoVisibleDuelViewModelBrokerResultV1,
) -> Result<&crate::MtgoPlayerVisibleDuelStateV1, MtgoContractErrorV1> {
    match result {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { decision } => {
            Ok(&decision.current_state)
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { selection } => {
            Ok(&selection.current_state)
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
            selection,
        }
        | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
            selection,
        } => Ok(&selection.current_state),
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
            selection,
        } => Ok(&selection.current_state),
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { selection } => {
            Ok(&selection.current_state)
        }
        MtgoVisibleDuelViewModelBrokerResultV1::Abstained { .. } => Err(error_v1(
            "visible_combat_transition_abstained",
            "an abstention cannot confirm a submitted combat operation",
        )),
    }
}

fn validate_same_attacker_transition_universe_v1(
    before: &crate::MtgoPlayerVisibleAttackerSelectionInputV1,
    after: &crate::MtgoPlayerVisibleAttackerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    if before.ordered_candidates.len() != after.ordered_candidates.len()
        || before
            .ordered_candidates
            .iter()
            .zip(&after.ordered_candidates)
            .any(|(left, right)| left.attacker != right.attacker)
        || before.unique_visible_enabled_done_control != after.unique_visible_enabled_done_control
    {
        return Err(error_v1(
            "visible_combat_transition_attacker_universe",
            "the visible attacker order and completion control must remain exact during one toggle",
        ));
    }
    let mut before_state = before.current_state.clone();
    let mut after_state = after.current_state.clone();
    before_state.combat.ordered_attackers.clear();
    after_state.combat.ordered_attackers.clear();
    if before_state != after_state {
        return Err(error_v1(
            "visible_combat_transition_attacker_state",
            "only the rendered attacker set may change during one attacker toggle",
        ));
    }
    Ok(())
}

fn validate_attacker_selection_consistency_v1(
    selection: &crate::MtgoPlayerVisibleAttackerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    let selected = selection
        .ordered_candidates
        .iter()
        .filter(|candidate| candidate.currently_attacking)
        .map(|candidate| candidate.attacker)
        .collect::<Vec<_>>();
    if selection.current_state.combat.ordered_attackers != selected {
        return Err(error_v1(
            "visible_combat_transition_attacker_consistency",
            "the rendered candidate flags and ordered attacker state disagree",
        ));
    }
    Ok(())
}

fn validate_same_single_blocker_transition_universe_v1(
    before: &crate::MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
    after: &crate::MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    if before.attacker != after.attacker
        || before.ordered_candidates.len() != after.ordered_candidates.len()
        || before
            .ordered_candidates
            .iter()
            .zip(&after.ordered_candidates)
            .any(|(left, right)| left.blocker != right.blocker)
        || before.unique_visible_enabled_done_control != after.unique_visible_enabled_done_control
    {
        return Err(error_v1(
            "visible_combat_transition_single_blocker_universe",
            "the attacker, blocker order, and completion control must remain exact during one assignment",
        ));
    }
    let mut before_state = before.current_state.clone();
    let mut after_state = after.current_state.clone();
    before_state.combat.blocker_assignments.clear();
    after_state.combat.blocker_assignments.clear();
    if before_state != after_state {
        return Err(error_v1(
            "visible_combat_transition_single_blocker_state",
            "only the rendered blocker assignments may change during one single-attacker blocker step",
        ));
    }
    Ok(())
}

fn require_attacker_plan_reconciled_v1(
    prepared: &CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1,
    current: &crate::MtgoPlayerVisibleAttackerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    let plan = prepared.attacker_execution_plan_v1().ok_or_else(|| {
        error_v1(
            "visible_combat_transition_attacker_plan",
            "attacker completion lost its exact prepared attacker plan",
        )
    })?;
    let current_bytes = serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection {
            selection: Box::new(current.clone()),
        },
    )
    .map_err(|error| error_v1("visible_combat_transition_attacker_plan", error.to_string()))?;
    let step = reobserve_player_visible_attacker_execution_step_v1(plan, &current_bytes)?;
    if step.operation_v1() != MtgoPlayerVisibleAttackerExecutionOperationV1::Done {
        return Err(error_v1(
            "visible_combat_transition_attacker_plan",
            "attacker completion was submitted before the visible selection matched the model plan",
        ));
    }
    Ok(())
}

fn require_single_blocker_plan_reconciled_v1(
    prepared: &CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1,
    current: &crate::MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    let plan = prepared
        .single_attacker_blocker_execution_plan_v1()
        .ok_or_else(|| {
            error_v1(
                "visible_combat_transition_single_blocker_plan",
                "blocker completion lost its exact prepared single-attacker plan",
            )
        })?;
    let current_bytes = serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
            selection: Box::new(current.clone()),
        },
    )
    .map_err(|error| {
        error_v1(
            "visible_combat_transition_single_blocker_plan",
            error.to_string(),
        )
    })?;
    let step =
        reobserve_player_visible_single_attacker_blocker_execution_step_v1(plan, &current_bytes)?;
    if step.operation_v1() != MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1::Done {
        return Err(error_v1(
            "visible_combat_transition_single_blocker_plan",
            "blocker completion was submitted before the visible assignments matched the model plan",
        ));
    }
    Ok(())
}

fn state_has_exact_blocker_assignment_v1(
    state: &crate::MtgoPlayerVisibleDuelStateV1,
    attacker: MtgoPlayerVisibleObjectRefV1,
    blocker: MtgoPlayerVisibleObjectRefV1,
) -> bool {
    state
        .combat
        .blocker_assignments
        .iter()
        .filter(|assignment| assignment.attacker == attacker)
        .flat_map(|assignment| assignment.ordered_blockers.iter())
        .filter(|candidate| **candidate == blocker)
        .count()
        == 1
}

fn assignment_pairs_v1(
    state: &crate::MtgoPlayerVisibleDuelStateV1,
) -> Vec<(MtgoPlayerVisibleObjectRefV1, MtgoPlayerVisibleObjectRefV1)> {
    state
        .combat
        .blocker_assignments
        .iter()
        .flat_map(|assignment| {
            assignment
                .ordered_blockers
                .iter()
                .map(move |blocker| (assignment.attacker, *blocker))
        })
        .collect()
}

fn commitment_parts_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        add_part_v1(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

/// Confirms the exact operation against a different, later complete sanitized
/// result. Merely changing bytes is insufficient. The intended public combat
/// fact must change, all unrelated visible state must remain exact for an
/// intermediate step, and completion must visibly leave the selection prompt
/// or set the corresponding declared flag.
pub fn confirm_player_visible_combat_execution_transition_v1(
    step: CheckedUntrustedMtgoPlayerVisibleCombatExecutionStepV1,
    exact_after_producer_result: &[u8],
) -> Result<CheckedUntrustedMtgoPlayerVisibleCombatTransitionV1, MtgoContractErrorV1> {
    let after = parse_and_validate_visible_duel_producer_result_v1(exact_after_producer_result)?;
    let after_selection_sha256 = sha256_v1(exact_after_producer_result);
    if after_selection_sha256 == step.current_selection_sha256 {
        return Err(error_v1(
            "visible_combat_transition_stale",
            "combat confirmation requires a different fresh sanitized result",
        ));
    }
    let progress = validate_combat_visible_transition_v1(
        &step.prepared,
        &step.current,
        &after,
        step.operation,
    )?;
    let operation_commitment_sha256 = operation_commitment_sha256_v1(step.operation);
    let progress_text = transition_progress_text_v1(progress);
    let confirmation_commitment_sha256 = commitment_parts_v1(
        COMBAT_VISIBLE_TRANSITION_DOMAIN_V1,
        &[
            step.execution_step_commitment_sha256.as_bytes(),
            step.current_selection_sha256.as_bytes(),
            after_selection_sha256.as_bytes(),
            operation_commitment_sha256.as_bytes(),
            progress_text,
            b"exact_intended_player_visible_transition_confirmed",
        ],
    );
    let prepared = (progress == MtgoPlayerVisibleCombatTransitionProgressV1::ContinueSamePlan)
        .then_some(step.prepared);
    Ok(CheckedUntrustedMtgoPlayerVisibleCombatTransitionV1 {
        prepared,
        progress,
        operation: step.operation,
        before_selection_sha256: step.current_selection_sha256,
        after_selection_sha256,
        confirmation_commitment_sha256,
    })
}

fn operation_commitment_sha256_v1(
    operation: MtgoPlayerVisibleCombatSubmittedOperationV1,
) -> String {
    match operation {
        MtgoPlayerVisibleCombatSubmittedOperationV1::ToggleAttacker {
            attacker,
            select_for_attack,
        } => commitment_parts_v1(
            b"mtgo-player-visible-combat-operation-toggle-attacker-v1",
            &[
                attacker.visible_ordinal.to_string().as_bytes(),
                if select_for_attack {
                    b"select"
                } else {
                    b"deselect"
                },
            ],
        ),
        MtgoPlayerVisibleCombatSubmittedOperationV1::FinishAttackers => commitment_parts_v1(
            b"mtgo-player-visible-combat-operation-finish-attackers-v1",
            &[],
        ),
        MtgoPlayerVisibleCombatSubmittedOperationV1::AddSingleAttackerBlocker { blocker } => {
            commitment_parts_v1(
                b"mtgo-player-visible-combat-operation-add-single-blocker-v1",
                &[blocker.visible_ordinal.to_string().as_bytes()],
            )
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::FinishSingleAttackerBlockers => {
            commitment_parts_v1(
                b"mtgo-player-visible-combat-operation-finish-single-blockers-v1",
                &[],
            )
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::FinishMultiAttackerBlockers => {
            commitment_parts_v1(
                b"mtgo-player-visible-combat-operation-finish-multi-blockers-v1",
                &[],
            )
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::ChooseMultiAttackerBlocker { blocker } => {
            commitment_parts_v1(
                b"mtgo-player-visible-combat-operation-choose-blocker-v1",
                &[blocker.visible_ordinal.to_string().as_bytes()],
            )
        }
        MtgoPlayerVisibleCombatSubmittedOperationV1::ChooseAttackerForBlocker {
            blocker,
            attacker,
        } => commitment_parts_v1(
            b"mtgo-player-visible-combat-operation-choose-attacker-v1",
            &[
                blocker.visible_ordinal.to_string().as_bytes(),
                attacker.visible_ordinal.to_string().as_bytes(),
            ],
        ),
    }
}

fn transition_progress_text_v1(
    progress: MtgoPlayerVisibleCombatTransitionProgressV1,
) -> &'static [u8] {
    match progress {
        MtgoPlayerVisibleCombatTransitionProgressV1::ContinueSamePlan => b"continue_same_plan",
        MtgoPlayerVisibleCombatTransitionProgressV1::AwaitFreshCombatModelDecision => {
            b"await_fresh_combat_model_decision"
        }
        MtgoPlayerVisibleCombatTransitionProgressV1::CombatDeclarationComplete => {
            b"combat_declaration_complete"
        }
    }
}

/// Scores any complete combat-specific producer result and immediately binds
/// the model choice to its coordinate-free execution contract. Ordinary duel
/// decisions remain on their existing scorer, and a mid-execution blocker
/// state cannot begin a second model deliberation.
pub fn score_and_prepare_strict_visible_combat_producer_result_v1<
    S: MtgoPlayerVisibleCombatScorerV1,
>(
    exact_producer_result: &[u8],
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1, MtgoContractErrorV1> {
    require_sha256_v1(deployment_commitment_sha256)?;
    let parsed = parse_and_validate_visible_duel_producer_result_v1(exact_producer_result)?;
    let exact_producer_result_sha256 = sha256_v1(exact_producer_result);

    let prepared = match parsed {
        MtgoVisibleDuelViewModelBrokerResultV1::Abstained { reason } => {
            return Ok(
                CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { reason },
            );
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { selection } => {
            let mut selection_commitments = Vec::with_capacity(selection.ordered_candidates.len());
            let mut progress = begin_player_visible_attacker_deliberation_v1(*selection)?;
            let plan = loop {
                match progress {
                    MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(plan) => break plan,
                    MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(deliberation) => {
                        let scored = score_and_advance_player_visible_attacker_deliberation_v1(
                            deliberation,
                            deployment_commitment_sha256,
                            scorer,
                        )?;
                        selection_commitments
                            .push(scored.selection_commitment_sha256_v1().to_owned());
                        progress = scored.into_progress_v1();
                    }
                }
            };
            let execution =
                prepare_player_visible_attacker_execution_plan_v1(plan, exact_producer_result)?;
            let execution_commitment = execution.plan_commitment_sha256_v1().to_owned();
            prepared_v1(
                MtgoPlayerVisiblePreparedCombatKindV1::AttackerPlan,
                exact_producer_result_sha256,
                deployment_commitment_sha256,
                selection_commitments,
                execution_commitment,
                MtgoPlayerVisiblePreparedCombatExecutionV1::Attacker(execution),
            )
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
            selection,
        } => {
            let mut selection_commitments = Vec::with_capacity(selection.ordered_candidates.len());
            let mut progress =
                begin_player_visible_single_attacker_blocker_deliberation_v1(*selection)?;
            let plan = loop {
                match progress {
                    MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Complete(
                        plan,
                    ) => {
                        break plan;
                    }
                    MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(
                        deliberation,
                    ) => {
                        let scored =
                            score_and_advance_player_visible_single_attacker_blocker_deliberation_v1(
                                deliberation,
                                deployment_commitment_sha256,
                                scorer,
                            )?;
                        selection_commitments
                            .push(scored.selection_commitment_sha256_v1().to_owned());
                        progress = scored.into_progress_v1();
                    }
                }
            };
            let execution = prepare_player_visible_single_attacker_blocker_execution_plan_v1(
                plan,
                exact_producer_result,
            )?;
            let execution_commitment = execution.plan_commitment_sha256_v1().to_owned();
            prepared_v1(
                MtgoPlayerVisiblePreparedCombatKindV1::SingleAttackerBlockerPlan,
                exact_producer_result_sha256,
                deployment_commitment_sha256,
                selection_commitments,
                execution_commitment,
                MtgoPlayerVisiblePreparedCombatExecutionV1::SingleAttackerBlocker(execution),
            )
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
            selection,
        } => {
            let choice = score_and_select_player_visible_multi_attacker_blocker_v1(
                *selection,
                deployment_commitment_sha256,
                scorer,
            )?;
            let selection_commitment = choice.selection_commitment_sha256_v1().to_owned();
            let execution = prepare_player_visible_multi_attacker_blocker_execution_step_v1(
                &choice,
                exact_producer_result,
            )?;
            let execution_commitment = execution.execution_step_commitment_sha256_v1().to_owned();
            prepared_v1(
                MtgoPlayerVisiblePreparedCombatKindV1::MultiAttackerBlockerStep,
                exact_producer_result_sha256,
                deployment_commitment_sha256,
                vec![selection_commitment],
                execution_commitment,
                MtgoPlayerVisiblePreparedCombatExecutionV1::MultiAttackerBlocker(execution),
            )
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { selection } => {
            let choice = score_and_select_player_visible_blocker_target_v1(
                *selection,
                deployment_commitment_sha256,
                scorer,
            )?;
            let selection_commitment = choice.selection_commitment_sha256_v1().to_owned();
            let execution = prepare_player_visible_multi_attacker_blocker_execution_step_v1(
                &choice,
                exact_producer_result,
            )?;
            let execution_commitment = execution.execution_step_commitment_sha256_v1().to_owned();
            prepared_v1(
                MtgoPlayerVisiblePreparedCombatKindV1::MultiAttackerBlockerStep,
                exact_producer_result_sha256,
                deployment_commitment_sha256,
                vec![selection_commitment],
                execution_commitment,
                MtgoPlayerVisiblePreparedCombatExecutionV1::MultiAttackerBlocker(execution),
            )
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
            ..
        } => {
            return Err(error_v1(
                "visible_combat_scoring_execution_state",
                "a mid-execution visible blocker state cannot begin a second model deliberation",
            ));
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { .. } => {
            return Err(error_v1(
                "visible_combat_scoring_result_kind",
                "an ordinary visible duel decision must use the ordinary player-visible scorer",
            ));
        }
    };

    Ok(CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared))
}

fn prepared_v1(
    kind: MtgoPlayerVisiblePreparedCombatKindV1,
    exact_producer_result_sha256: String,
    deployment_commitment_sha256: &str,
    model_selection_commitments_sha256: Vec<String>,
    execution_commitment_sha256: String,
    execution: MtgoPlayerVisiblePreparedCombatExecutionV1,
) -> CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1 {
    let kind_text = match kind {
        MtgoPlayerVisiblePreparedCombatKindV1::AttackerPlan => b"attacker_plan".as_slice(),
        MtgoPlayerVisiblePreparedCombatKindV1::SingleAttackerBlockerPlan => {
            b"single_attacker_blocker_plan".as_slice()
        }
        MtgoPlayerVisiblePreparedCombatKindV1::MultiAttackerBlockerStep => {
            b"multi_attacker_blocker_step".as_slice()
        }
    };
    let mut hasher = Sha256::new();
    add_part_v1(&mut hasher, COMBAT_SCORING_BRIDGE_DOMAIN_V1);
    add_part_v1(&mut hasher, kind_text);
    add_part_v1(&mut hasher, exact_producer_result_sha256.as_bytes());
    add_part_v1(&mut hasher, deployment_commitment_sha256.as_bytes());
    add_part_v1(
        &mut hasher,
        &(model_selection_commitments_sha256.len() as u64).to_be_bytes(),
    );
    for commitment in &model_selection_commitments_sha256 {
        add_part_v1(&mut hasher, commitment.as_bytes());
    }
    add_part_v1(&mut hasher, execution_commitment_sha256.as_bytes());
    add_part_v1(
        &mut hasher,
        b"visible_equivalent_only_checked_untrusted_no_live_or_event_authority",
    );
    let bridge_commitment_sha256 = format!("{:x}", hasher.finalize());

    CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1 {
        kind,
        exact_producer_result_sha256,
        deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
        model_selection_commitments_sha256,
        execution_commitment_sha256,
        bridge_commitment_sha256,
        execution,
    }
}

fn require_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "visible_combat_scoring_deployment_commitment",
            "deployment commitment must be lowercase SHA-256",
        ));
    }
    Ok(())
}

fn sha256_v1(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn add_part_v1(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}
