use crate::{
    begin_player_visible_attacker_deliberation_v1,
    begin_player_visible_single_attacker_blocker_deliberation_v1,
    parse_and_validate_visible_duel_producer_result_v1,
    prepare_player_visible_attacker_execution_plan_v1,
    prepare_player_visible_multi_attacker_blocker_execution_step_v1,
    prepare_player_visible_single_attacker_blocker_execution_plan_v1,
    score_and_advance_player_visible_attacker_deliberation_v1,
    score_and_advance_player_visible_single_attacker_blocker_deliberation_v1,
    score_and_select_player_visible_blocker_target_v1,
    score_and_select_player_visible_multi_attacker_blocker_v1,
    CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1, MtgoContractErrorV1,
    MtgoPlayerVisibleAttackerDeliberationProgressV1, MtgoPlayerVisibleAttackerExecutionPlanV1,
    MtgoPlayerVisibleAttackerScorerV1, MtgoPlayerVisibleMultiAttackerBlockerScorerV1,
    MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1,
    MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1,
    MtgoPlayerVisibleSingleAttackerBlockerScorerV1,
    MtgoVisibleDuelViewModelBrokerAbstentionReasonV1, MtgoVisibleDuelViewModelBrokerResultV1,
};
use sha2::{Digest, Sha256};

const COMBAT_SCORING_BRIDGE_DOMAIN_V1: &[u8] = b"mtgo-player-visible-combat-scoring-bridge-v1";

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
