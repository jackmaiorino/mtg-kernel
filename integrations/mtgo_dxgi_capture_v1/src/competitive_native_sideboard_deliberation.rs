use crate::{
    competitive_native_sideboard_model_input_commitment_v1,
    competitive_native_sideboard_model_selection_commitment_v1,
    validate_competitive_native_sideboard_model_input_v1,
    validate_competitive_native_sideboard_model_selection_v1,
    MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1,
    MtgoCompetitiveNativeSideboardCardCountV1, MtgoCompetitiveNativeSideboardConfigurationV1,
    MtgoCompetitiveNativeSideboardModelInputV1, MtgoCompetitiveNativeSideboardModelSelectionV1,
    OpaqueMtgoCompetitiveNativeSideboardRequestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1: u32 = 1;
pub const MTGO_COMPETITIVE_NATIVE_SIDEBOARD_MAX_DECISIONS_V1: u8 = 64;

const DECISION_DOMAIN_V1: &[u8] = b"mtgo-competitive-native-sideboard-deliberation-decision-v1";
const RESPONSE_DOMAIN_V1: &[u8] = b"mtgo-competitive-native-sideboard-deliberation-response-v1";
const RECEIPT_DOMAIN_V1: &[u8] = b"mtgo-competitive-native-sideboard-deliberation-receipt-v1";
const TRACE_DOMAIN_V1: &[u8] = b"mtgo-competitive-native-sideboard-deliberation-trace-v1";
const MIN_MAINBOARD_CARDS_V1: u32 = 60;
const MAX_SIDEBOARD_CARDS_V1: u32 = 15;

/// One pure-local sideboard action. The visible card name is a stable identity
/// within the exact player-known deck inventory. No adapter card ID, region,
/// coordinate, event session, or input capability is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum MtgoCompetitiveNativeSideboardDeliberationActionV1 {
    MoveOneToMainboard { visible_card_name: String },
    MoveOneToSideboard { visible_card_name: String },
    SubmitConfiguration,
}

/// One exact model-visible decision in the bounded local deliberation. Actions
/// are ordered first by action kind, then by canonical visible card name, with
/// SubmitConfiguration last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativeSideboardDeliberationDecisionV1 {
    pub schema_version: u32,
    pub decision_number: u8,
    pub model_input_commitment_sha256: String,
    pub deployment_commitment_sha256: String,
    pub prior_trace_commitment_sha256: String,
    pub candidate_configuration: MtgoCompetitiveNativeSideboardConfigurationV1,
    pub ordered_actions: Vec<MtgoCompetitiveNativeSideboardDeliberationActionV1>,
    pub decision_commitment_sha256: String,
}

/// Transport-only output expected from a future checkpoint-bound sequential
/// sideboard head. It selects no target directly. The adapter deterministically
/// chooses the first maximum logit in the exact ordered action vector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 {
    pub schema_version: u32,
    pub decision_commitment_sha256: String,
    pub deployment_commitment_sha256: String,
    pub ordered_logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
}

/// Semantic transport seam for a future native checkpoint head or an offline
/// test double. Implementing the trait does not grant live authority.
pub trait MtgoCompetitiveNativeSideboardDeliberationScorerV1 {
    fn score_sideboard_deliberation_v1(
        &mut self,
        decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
    ) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativeSideboardDeliberationStepReceiptV1 {
    pub schema_version: u32,
    pub decision_number: u8,
    pub decision_commitment_sha256: String,
    pub deployment_commitment_sha256: String,
    pub selected_index: u32,
    pub selected_action: MtgoCompetitiveNativeSideboardDeliberationActionV1,
    pub ordered_logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
    pub scorer_response_commitment_sha256: String,
    pub step_receipt_commitment_sha256: String,
}

/// Move-only pure-local deliberation. It owns no MTGO runtime or input object
/// and cannot be serialized, cloned, or forged from ordinary response data.
pub struct OpaqueMtgoCompetitiveNativeSideboardDeliberationV1 {
    model_input: MtgoCompetitiveNativeSideboardModelInputV1,
    model_input_commitment_sha256: String,
    deployment_commitment_sha256: String,
    candidate_configuration: MtgoCompetitiveNativeSideboardConfigurationV1,
    decisions_consumed: u8,
    trace_commitment_sha256: String,
}

impl OpaqueMtgoCompetitiveNativeSideboardDeliberationV1 {
    pub fn decision_v1(
        &self,
    ) -> Result<MtgoCompetitiveNativeSideboardDeliberationDecisionV1, String> {
        if self.decisions_consumed >= MTGO_COMPETITIVE_NATIVE_SIDEBOARD_MAX_DECISIONS_V1 {
            return Err("native sideboard deliberation exhausted its decision bound".to_owned());
        }
        build_decision_v1(self)
    }

    pub fn decisions_consumed_v1(&self) -> u8 {
        self.decisions_consumed
    }

    pub fn trace_commitment_sha256_v1(&self) -> &str {
        &self.trace_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

/// Opaque proof that the checkpoint-style scorer selected SubmitConfiguration
/// after zero or more locally applied moves. The final target remains private
/// to the adapter until a future production binder consumes this object.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSubmittedCompetitiveNativeSideboardSelectionV1;
/// fn cannot_act(value: OpaqueMtgoSubmittedCompetitiveNativeSideboardSelectionV1) {
///     let _ = value.target_configuration_v1();
///     let _ = value.submit_sideboard();
///     let _ = value.event_session();
/// }
/// ```
pub struct OpaqueMtgoSubmittedCompetitiveNativeSideboardSelectionV1 {
    _selection: MtgoCompetitiveNativeSideboardModelSelectionV1,
    model_input_commitment_sha256: String,
    deployment_commitment_sha256: String,
    model_selection_commitment_sha256: String,
    trace_commitment_sha256: String,
    decisions_consumed: u8,
    no_changes_selected: bool,
}

/// Checked-untrusted move-only result of importing the exact retained
/// completed-game history and running every local decision through the same
/// scorer instance. It retains the source event request and submitted result
/// privately but exposes no recovery path. A future concrete checkpoint
/// implementation must create a separate production-owned proof type.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoScoredCompetitiveNativeSideboardDeliberationRequestV1;
/// fn cannot_act(value: OpaqueMtgoScoredCompetitiveNativeSideboardDeliberationRequestV1) {
///     let _ = value.into_event_session();
///     let _ = value.target_configuration_v1();
///     let _ = value.submit_sideboard();
/// }
/// ```
pub struct OpaqueMtgoScoredCompetitiveNativeSideboardDeliberationRequestV1 {
    _request: OpaqueMtgoCompetitiveNativeSideboardRequestV1,
    submission: OpaqueMtgoSubmittedCompetitiveNativeSideboardSelectionV1,
}

impl OpaqueMtgoScoredCompetitiveNativeSideboardDeliberationRequestV1 {
    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        self.submission.model_input_commitment_sha256_v1()
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        self.submission.deployment_commitment_sha256_v1()
    }

    pub fn model_selection_commitment_sha256_v1(&self) -> &str {
        self.submission.model_selection_commitment_sha256_v1()
    }

    pub fn trace_commitment_sha256_v1(&self) -> &str {
        self.submission.trace_commitment_sha256_v1()
    }

    pub fn decisions_consumed_v1(&self) -> u8 {
        self.submission.decisions_consumed_v1()
    }

    pub fn no_changes_selected_v1(&self) -> bool {
        self.submission.no_changes_selected_v1()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoSubmittedCompetitiveNativeSideboardSelectionV1 {
    #[cfg(test)]
    pub(crate) fn selection_v1(&self) -> &MtgoCompetitiveNativeSideboardModelSelectionV1 {
        &self._selection
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.model_input_commitment_sha256
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.deployment_commitment_sha256
    }

    pub fn model_selection_commitment_sha256_v1(&self) -> &str {
        &self.model_selection_commitment_sha256
    }

    pub fn trace_commitment_sha256_v1(&self) -> &str {
        &self.trace_commitment_sha256
    }

    pub fn decisions_consumed_v1(&self) -> u8 {
        self.decisions_consumed
    }

    pub fn no_changes_selected_v1(&self) -> bool {
        self.no_changes_selected
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// One consumed decision either continues the local candidate or finishes it.
/// Neither branch contains an event session or live input capability.
pub enum MtgoCompetitiveNativeSideboardDeliberationAdvanceV1 {
    Continue {
        deliberation: OpaqueMtgoCompetitiveNativeSideboardDeliberationV1,
        receipt: MtgoCompetitiveNativeSideboardDeliberationStepReceiptV1,
    },
    Submitted {
        submission: OpaqueMtgoSubmittedCompetitiveNativeSideboardSelectionV1,
        receipt: MtgoCompetitiveNativeSideboardDeliberationStepReceiptV1,
    },
}

pub fn begin_competitive_native_sideboard_deliberation_v1(
    model_input: MtgoCompetitiveNativeSideboardModelInputV1,
    deployment_commitment_sha256: &str,
) -> Result<OpaqueMtgoCompetitiveNativeSideboardDeliberationV1, String> {
    validate_competitive_native_sideboard_model_input_v1(&model_input)?;
    require_sha256_v1(
        deployment_commitment_sha256,
        "native sideboard deliberation deployment commitment",
    )?;
    let model_input_commitment_sha256 =
        competitive_native_sideboard_model_input_commitment_v1(&model_input)?;
    let trace_commitment_sha256 = commitment_v1(
        TRACE_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            deployment_commitment_sha256.as_bytes(),
            b"begin_pure_local_player_visible_deliberation_no_mtgo_input",
        ],
    );
    let candidate_configuration = model_input.current_configuration.clone();
    Ok(OpaqueMtgoCompetitiveNativeSideboardDeliberationV1 {
        model_input,
        model_input_commitment_sha256,
        deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
        candidate_configuration,
        decisions_consumed: 0,
        trace_commitment_sha256,
    })
}

pub fn advance_competitive_native_sideboard_deliberation_v1<
    S: MtgoCompetitiveNativeSideboardDeliberationScorerV1,
>(
    mut deliberation: OpaqueMtgoCompetitiveNativeSideboardDeliberationV1,
    scorer: &mut S,
) -> Result<MtgoCompetitiveNativeSideboardDeliberationAdvanceV1, String> {
    let decision = deliberation.decision_v1()?;
    let response = scorer.score_sideboard_deliberation_v1(&decision)?;
    validate_score_response_v1(&decision, &response)?;
    let selected_index = first_argmax_v1(&response.ordered_logits_f32_bits);
    let selected_action = decision.ordered_actions[selected_index].clone();
    let response_json = serde_json::to_vec(&response)
        .map_err(|error| format!("serialize native sideboard deliberation response: {error}"))?;
    let scorer_response_commitment_sha256 = commitment_v1(RESPONSE_DOMAIN_V1, &[&response_json]);
    let selected_index_u32 = u32::try_from(selected_index)
        .map_err(|_| "native sideboard selected index overflow".to_owned())?;
    let selected_action_json = serde_json::to_vec(&selected_action)
        .map_err(|error| format!("serialize native sideboard selected action: {error}"))?;
    let step_receipt_commitment_sha256 = commitment_v1(
        RECEIPT_DOMAIN_V1,
        &[
            decision.decision_commitment_sha256.as_bytes(),
            scorer_response_commitment_sha256.as_bytes(),
            &selected_index_u32.to_le_bytes(),
            &selected_action_json,
            b"deterministic_first_argmax_pure_local_no_input",
        ],
    );
    let receipt = MtgoCompetitiveNativeSideboardDeliberationStepReceiptV1 {
        schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
        decision_number: decision.decision_number,
        decision_commitment_sha256: decision.decision_commitment_sha256,
        deployment_commitment_sha256: deliberation.deployment_commitment_sha256.clone(),
        selected_index: selected_index_u32,
        selected_action: selected_action.clone(),
        ordered_logits_f32_bits: response.ordered_logits_f32_bits,
        value_f32_bits: response.value_f32_bits,
        scorer_response_commitment_sha256,
        step_receipt_commitment_sha256: step_receipt_commitment_sha256.clone(),
    };
    deliberation.decisions_consumed = deliberation
        .decisions_consumed
        .checked_add(1)
        .ok_or("native sideboard deliberation decision count overflow")?;
    deliberation.trace_commitment_sha256 = commitment_v1(
        TRACE_DOMAIN_V1,
        &[
            deliberation.trace_commitment_sha256.as_bytes(),
            step_receipt_commitment_sha256.as_bytes(),
        ],
    );

    match selected_action {
        MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration => {
            let selection = MtgoCompetitiveNativeSideboardModelSelectionV1 {
                target_configuration: deliberation.candidate_configuration.clone(),
            };
            validate_competitive_native_sideboard_model_selection_v1(
                &deliberation.model_input,
                &selection,
            )?;
            let model_selection_commitment_sha256 =
                competitive_native_sideboard_model_selection_commitment_v1(
                    &deliberation.model_input,
                    &selection,
                )?;
            let no_changes_selected =
                selection.target_configuration == deliberation.model_input.current_configuration;
            Ok(
                MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Submitted {
                    submission: OpaqueMtgoSubmittedCompetitiveNativeSideboardSelectionV1 {
                        _selection: selection,
                        model_input_commitment_sha256: deliberation.model_input_commitment_sha256,
                        deployment_commitment_sha256: deliberation.deployment_commitment_sha256,
                        model_selection_commitment_sha256,
                        trace_commitment_sha256: deliberation.trace_commitment_sha256,
                        decisions_consumed: deliberation.decisions_consumed,
                        no_changes_selected,
                    },
                    receipt,
                },
            )
        }
        action => {
            let mut candidate = deliberation.candidate_configuration.clone();
            apply_local_action_v1(&mut candidate, &action)?;
            deliberation.candidate_configuration = candidate;
            if deliberation.decisions_consumed >= MTGO_COMPETITIVE_NATIVE_SIDEBOARD_MAX_DECISIONS_V1
            {
                return Err(
                    "native sideboard deliberation reached its bound without SubmitConfiguration"
                        .to_owned(),
                );
            }
            Ok(
                MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Continue {
                    deliberation,
                    receipt,
                },
            )
        }
    }
}

/// Imports the source request's complete earlier-game player-visible history
/// into the same scorer instance that handles every local sideboard decision.
/// The entire run is bounded and emits no MTGO input. Only the opaque submitted
/// result retains the exact event request.
pub fn score_competitive_native_sideboard_request_deliberation_v1<S>(
    request: OpaqueMtgoCompetitiveNativeSideboardRequestV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoScoredCompetitiveNativeSideboardDeliberationRequestV1, String>
where
    S: MtgoCompetitiveNativeSideboardDeliberationScorerV1
        + MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1<Output = ()>,
{
    request
        .visit_completed_match_history_v1(scorer)
        .map_err(|error| format!("import completed visible games for sideboard scorer: {error}"))?;
    let model_input = request.model_input_v1().clone();
    let mut deliberation = begin_competitive_native_sideboard_deliberation_v1(
        model_input,
        deployment_commitment_sha256,
    )?;
    loop {
        match advance_competitive_native_sideboard_deliberation_v1(deliberation, scorer)? {
            MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Continue {
                deliberation: next,
                ..
            } => deliberation = next,
            MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Submitted {
                submission, ..
            } => {
                return Ok(
                    OpaqueMtgoScoredCompetitiveNativeSideboardDeliberationRequestV1 {
                        _request: request,
                        submission,
                    },
                )
            }
        }
    }
}

fn build_decision_v1(
    deliberation: &OpaqueMtgoCompetitiveNativeSideboardDeliberationV1,
) -> Result<MtgoCompetitiveNativeSideboardDeliberationDecisionV1, String> {
    crate::competitive_native_sideboard::validate_native_sideboard_configuration_v1(
        &deliberation.candidate_configuration,
    )?;
    let ordered_actions = ordered_actions_v1(&deliberation.candidate_configuration)?;
    let candidate_json = serde_json::to_vec(&deliberation.candidate_configuration)
        .map_err(|error| format!("serialize native sideboard deliberation candidate: {error}"))?;
    let actions_json = serde_json::to_vec(&ordered_actions)
        .map_err(|error| format!("serialize native sideboard deliberation actions: {error}"))?;
    let decision_number = deliberation
        .decisions_consumed
        .checked_add(1)
        .ok_or("native sideboard deliberation decision number overflow")?;
    let decision_commitment_sha256 = commitment_v1(
        DECISION_DOMAIN_V1,
        &[
            &[decision_number],
            deliberation.model_input_commitment_sha256.as_bytes(),
            deliberation.deployment_commitment_sha256.as_bytes(),
            deliberation.trace_commitment_sha256.as_bytes(),
            &candidate_json,
            &actions_json,
        ],
    );
    Ok(MtgoCompetitiveNativeSideboardDeliberationDecisionV1 {
        schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
        decision_number,
        model_input_commitment_sha256: deliberation.model_input_commitment_sha256.clone(),
        deployment_commitment_sha256: deliberation.deployment_commitment_sha256.clone(),
        prior_trace_commitment_sha256: deliberation.trace_commitment_sha256.clone(),
        candidate_configuration: deliberation.candidate_configuration.clone(),
        ordered_actions,
        decision_commitment_sha256,
    })
}

fn ordered_actions_v1(
    configuration: &MtgoCompetitiveNativeSideboardConfigurationV1,
) -> Result<Vec<MtgoCompetitiveNativeSideboardDeliberationActionV1>, String> {
    let mainboard_total = partition_total_v1(&configuration.mainboard)?;
    let sideboard_total = partition_total_v1(&configuration.sideboard)?;
    let mut actions = configuration
        .sideboard
        .iter()
        .map(
            |card| MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToMainboard {
                visible_card_name: card.visible_card_name.clone(),
            },
        )
        .collect::<Vec<_>>();
    if mainboard_total > MIN_MAINBOARD_CARDS_V1 && sideboard_total < MAX_SIDEBOARD_CARDS_V1 {
        actions.extend(configuration.mainboard.iter().map(|card| {
            MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToSideboard {
                visible_card_name: card.visible_card_name.clone(),
            }
        }));
    }
    actions.push(MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration);
    Ok(actions)
}

fn apply_local_action_v1(
    configuration: &mut MtgoCompetitiveNativeSideboardConfigurationV1,
    action: &MtgoCompetitiveNativeSideboardDeliberationActionV1,
) -> Result<(), String> {
    match action {
        MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToMainboard {
            visible_card_name,
        } => move_one_v1(
            &mut configuration.sideboard,
            &mut configuration.mainboard,
            visible_card_name,
        )?,
        MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToSideboard {
            visible_card_name,
        } => move_one_v1(
            &mut configuration.mainboard,
            &mut configuration.sideboard,
            visible_card_name,
        )?,
        MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration => {
            return Err("SubmitConfiguration cannot be applied as a local move".to_owned())
        }
    }
    crate::competitive_native_sideboard::validate_native_sideboard_configuration_v1(configuration)
}

fn move_one_v1(
    source: &mut Vec<MtgoCompetitiveNativeSideboardCardCountV1>,
    destination: &mut Vec<MtgoCompetitiveNativeSideboardCardCountV1>,
    visible_card_name: &str,
) -> Result<(), String> {
    let source_index = source
        .binary_search_by(|card| card.visible_card_name.as_str().cmp(visible_card_name))
        .map_err(|_| "native sideboard local move source card is absent".to_owned())?;
    if source[source_index].count == 1 {
        source.remove(source_index);
    } else {
        source[source_index].count -= 1;
    }
    match destination
        .binary_search_by(|card| card.visible_card_name.as_str().cmp(visible_card_name))
    {
        Ok(index) => {
            destination[index].count = destination[index]
                .count
                .checked_add(1)
                .ok_or("native sideboard local move count overflow")?;
        }
        Err(index) => destination.insert(
            index,
            MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: visible_card_name.to_owned(),
                count: 1,
            },
        ),
    }
    Ok(())
}

fn validate_score_response_v1(
    decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
    response: &MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1,
) -> Result<(), String> {
    if response.schema_version != MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1
        || response.decision_commitment_sha256 != decision.decision_commitment_sha256
        || response.deployment_commitment_sha256 != decision.deployment_commitment_sha256
    {
        return Err(
            "native sideboard deliberation score changed the exact decision or deployment"
                .to_owned(),
        );
    }
    if response.ordered_logits_f32_bits.len() != decision.ordered_actions.len()
        || response.ordered_logits_f32_bits.is_empty()
        || response
            .ordered_logits_f32_bits
            .iter()
            .any(|bits| !f32::from_bits(*bits).is_finite())
        || !f32::from_bits(response.value_f32_bits).is_finite()
    {
        return Err(
            "native sideboard deliberation score has an invalid finite ordered vector".to_owned(),
        );
    }
    Ok(())
}

fn first_argmax_v1(logits: &[u32]) -> usize {
    let mut best_index = 0;
    let mut best_value = f32::from_bits(logits[0]);
    for (index, bits) in logits.iter().enumerate().skip(1) {
        let value = f32::from_bits(*bits);
        if value > best_value {
            best_index = index;
            best_value = value;
        }
    }
    best_index
}

fn partition_total_v1(cards: &[MtgoCompetitiveNativeSideboardCardCountV1]) -> Result<u32, String> {
    cards.iter().try_fold(0_u32, |total, card| {
        total
            .checked_add(u32::from(card.count))
            .ok_or_else(|| "native sideboard deliberation partition count overflow".to_owned())
    })
}

fn require_sha256_v1(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} is not a lowercase SHA-256"));
    }
    Ok(())
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_le_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const DEPLOYMENT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn card(name: &str, count: u16) -> MtgoCompetitiveNativeSideboardCardCountV1 {
        MtgoCompetitiveNativeSideboardCardCountV1 {
            visible_card_name: name.to_owned(),
            count,
        }
    }

    fn input_v1(
        mainboard: Vec<MtgoCompetitiveNativeSideboardCardCountV1>,
        sideboard: Vec<MtgoCompetitiveNativeSideboardCardCountV1>,
    ) -> MtgoCompetitiveNativeSideboardModelInputV1 {
        MtgoCompetitiveNativeSideboardModelInputV1 {
            next_game_number: 2,
            acting_player_games_won: 1,
            opponent_games_won: 0,
            current_configuration: MtgoCompetitiveNativeSideboardConfigurationV1 {
                mainboard,
                sideboard,
            },
        }
    }

    struct ScriptedScorerV1 {
        actions: VecDeque<MtgoCompetitiveNativeSideboardDeliberationActionV1>,
    }

    impl MtgoCompetitiveNativeSideboardDeliberationScorerV1 for ScriptedScorerV1 {
        fn score_sideboard_deliberation_v1(
            &mut self,
            decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
        ) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String> {
            let wanted = self
                .actions
                .pop_front()
                .ok_or("scripted sideboard scorer exhausted")?;
            let index = decision
                .ordered_actions
                .iter()
                .position(|action| action == &wanted)
                .ok_or("scripted sideboard action is not legal")?;
            let mut logits = vec![0.0_f32.to_bits(); decision.ordered_actions.len()];
            logits[index] = 1.0_f32.to_bits();
            Ok(MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 {
                schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
                decision_commitment_sha256: decision.decision_commitment_sha256.clone(),
                deployment_commitment_sha256: decision.deployment_commitment_sha256.clone(),
                ordered_logits_f32_bits: logits,
                value_f32_bits: 0.25_f32.to_bits(),
            })
        }
    }

    struct FirstActionScorerV1;

    impl MtgoCompetitiveNativeSideboardDeliberationScorerV1 for FirstActionScorerV1 {
        fn score_sideboard_deliberation_v1(
            &mut self,
            decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
        ) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String> {
            let mut logits = vec![0.0_f32.to_bits(); decision.ordered_actions.len()];
            logits[0] = 1.0_f32.to_bits();
            Ok(MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 {
                schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
                decision_commitment_sha256: decision.decision_commitment_sha256.clone(),
                deployment_commitment_sha256: decision.deployment_commitment_sha256.clone(),
                ordered_logits_f32_bits: logits,
                value_f32_bits: 0.0_f32.to_bits(),
            })
        }
    }

    #[test]
    fn unchanged_submission_is_an_explicit_model_decision() {
        let input = input_v1(vec![card("Alpha", 60)], vec![card("Beta", 15)]);
        let deliberation =
            begin_competitive_native_sideboard_deliberation_v1(input, DEPLOYMENT).unwrap();
        assert_eq!(
            deliberation.decision_v1().unwrap().ordered_actions,
            vec![
                MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToMainboard {
                    visible_card_name: "Beta".to_owned()
                },
                MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration,
            ]
        );
        let mut scorer = ScriptedScorerV1 {
            actions: VecDeque::from([
                MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration,
            ]),
        };
        let advance =
            advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut scorer)
                .unwrap();
        let MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Submitted {
            submission,
            receipt,
        } = advance
        else {
            panic!("expected explicit unchanged submission")
        };
        assert!(submission.no_changes_selected_v1());
        assert_eq!(submission.decisions_consumed_v1(), 1);
        assert_eq!(receipt.selected_index, 1);
        assert!(!submission.safe_for_live_input_v1());
        assert!(!submission.permits_sideboard_submission_v1());
    }

    #[test]
    fn changed_target_requires_ordered_local_moves_then_submit() {
        let input = input_v1(vec![card("Alpha", 60)], vec![card("Beta", 15)]);
        let original = input.clone();
        let mut scorer = ScriptedScorerV1 {
            actions: VecDeque::from([
                MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToMainboard {
                    visible_card_name: "Beta".to_owned(),
                },
                MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToSideboard {
                    visible_card_name: "Alpha".to_owned(),
                },
                MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration,
            ]),
        };
        let deliberation =
            begin_competitive_native_sideboard_deliberation_v1(input, DEPLOYMENT).unwrap();
        let MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Continue { deliberation, .. } =
            advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut scorer)
                .unwrap()
        else {
            panic!("expected first local move")
        };
        assert_eq!(
            deliberation.decision_v1().unwrap().ordered_actions,
            vec![
                MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToMainboard {
                    visible_card_name: "Beta".to_owned()
                },
                MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToSideboard {
                    visible_card_name: "Alpha".to_owned()
                },
                MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToSideboard {
                    visible_card_name: "Beta".to_owned()
                },
                MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration,
            ]
        );
        let MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Continue { deliberation, .. } =
            advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut scorer)
                .unwrap()
        else {
            panic!("expected second local move")
        };
        let MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Submitted { submission, .. } =
            advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut scorer)
                .unwrap()
        else {
            panic!("expected changed submission")
        };
        assert!(!submission.no_changes_selected_v1());
        assert_eq!(submission.decisions_consumed_v1(), 3);
        assert_eq!(
            submission.selection_v1().target_configuration,
            MtgoCompetitiveNativeSideboardConfigurationV1 {
                mainboard: vec![card("Alpha", 59), card("Beta", 1)],
                sideboard: vec![card("Alpha", 1), card("Beta", 14)],
            }
        );
        validate_competitive_native_sideboard_model_selection_v1(
            &original,
            submission.selection_v1(),
        )
        .unwrap();
    }

    #[test]
    fn equal_logits_select_first_action_and_decision_withholds_hidden_fields() {
        struct TiedScorerV1;
        impl MtgoCompetitiveNativeSideboardDeliberationScorerV1 for TiedScorerV1 {
            fn score_sideboard_deliberation_v1(
                &mut self,
                decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
            ) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String>
            {
                Ok(MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 {
                    schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
                    decision_commitment_sha256: decision.decision_commitment_sha256.clone(),
                    deployment_commitment_sha256: decision.deployment_commitment_sha256.clone(),
                    ordered_logits_f32_bits: vec![
                        0.0_f32.to_bits();
                        decision.ordered_actions.len()
                    ],
                    value_f32_bits: 0.0_f32.to_bits(),
                })
            }
        }

        let input = input_v1(vec![card("Alpha", 60)], vec![card("Beta", 15)]);
        let deliberation =
            begin_competitive_native_sideboard_deliberation_v1(input, DEPLOYMENT).unwrap();
        let decision_json = serde_json::to_string(&deliberation.decision_v1().unwrap()).unwrap();
        for forbidden in [
            "opponent_deck_id",
            "card_db_id",
            "event_id",
            "account",
            "rect",
            "coordinate",
            "process",
        ] {
            assert!(!decision_json.contains(forbidden));
        }
        let MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Continue { receipt, .. } =
            advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut TiedScorerV1)
                .unwrap()
        else {
            panic!("first canonical action is a local move")
        };
        assert_eq!(receipt.selected_index, 0);
        assert_eq!(
            receipt.selected_action,
            MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToMainboard {
                visible_card_name: "Beta".to_owned()
            }
        );
    }

    #[test]
    fn crossed_or_nonfinite_score_response_fails_closed() {
        struct BadScorerV1 {
            crossed: bool,
        }
        impl MtgoCompetitiveNativeSideboardDeliberationScorerV1 for BadScorerV1 {
            fn score_sideboard_deliberation_v1(
                &mut self,
                decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
            ) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String>
            {
                Ok(MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 {
                    schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
                    decision_commitment_sha256: if self.crossed {
                        "b".repeat(64)
                    } else {
                        decision.decision_commitment_sha256.clone()
                    },
                    deployment_commitment_sha256: decision.deployment_commitment_sha256.clone(),
                    ordered_logits_f32_bits: vec![
                        f32::NAN.to_bits();
                        decision.ordered_actions.len()
                    ],
                    value_f32_bits: 0.0_f32.to_bits(),
                })
            }
        }
        let input = input_v1(vec![card("Alpha", 60)], vec![card("Beta", 15)]);
        let deliberation =
            begin_competitive_native_sideboard_deliberation_v1(input.clone(), DEPLOYMENT).unwrap();
        let error = advance_competitive_native_sideboard_deliberation_v1(
            deliberation,
            &mut BadScorerV1 { crossed: true },
        )
        .err()
        .unwrap();
        assert!(error.contains("changed the exact decision"));

        let deliberation =
            begin_competitive_native_sideboard_deliberation_v1(input, DEPLOYMENT).unwrap();
        let error = advance_competitive_native_sideboard_deliberation_v1(
            deliberation,
            &mut BadScorerV1 { crossed: false },
        )
        .err()
        .unwrap();
        assert!(error.contains("invalid finite ordered vector"));
    }

    #[test]
    fn sixty_fourth_non_submit_decision_terminates_without_output() {
        let input = input_v1(vec![card("Alpha", 75)], vec![]);
        let mut deliberation =
            begin_competitive_native_sideboard_deliberation_v1(input, DEPLOYMENT).unwrap();
        let mut scorer = FirstActionScorerV1;
        for _ in 0..63 {
            let MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Continue {
                deliberation: next,
                ..
            } = advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut scorer)
                .unwrap()
            else {
                panic!("first-action scorer should not select SubmitConfiguration")
            };
            deliberation = next;
        }
        let error = advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut scorer)
            .err()
            .unwrap();
        assert!(error.contains("reached its bound without SubmitConfiguration"));
    }
}
