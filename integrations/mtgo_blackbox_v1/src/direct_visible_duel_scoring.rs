use crate::{
    parse_and_validate_visible_duel_producer_result_v1, MtgoContractErrorV1,
    MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MtgoPlayerVisibleDuelScoreResponseV1, MtgoPlayerVisibleDuelScorerV1,
    MtgoVisibleDuelViewModelBrokerAbstentionReasonV1, MtgoVisibleDuelViewModelBrokerResultV1,
    MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
};
use sha2::{Digest, Sha256};

const DIRECT_VISIBLE_SELECTION_DOMAIN_V1: &[u8] = b"mtgo-direct-visible-duel-scoring-selection-v1";
const DIRECT_VISIBLE_REFRESH_DOMAIN_V1: &[u8] = b"mtgo-direct-visible-duel-dispatch-refresh-v1";

/// Result of applying a player-visible-only scorer to the exact first outward
/// bytes released by the strict producer boundary. An abstention never calls
/// the scorer.
pub enum CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1 {
    Abstained {
        reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1,
    },
    Selected(Box<CheckedUntrustedMtgoDirectVisibleModelSelectionV1>),
}

/// One model selection bound to the SHA-256 of the exact strictly parsed
/// producer bytes. The scorer receives only the normalized player-visible
/// decision. This value has no client-action, input, event-entry, or spending
/// conversion.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDirectVisibleModelSelectionV1;
/// fn cannot_dispatch_or_recover_client_state(value: &CheckedUntrustedMtgoDirectVisibleModelSelectionV1) {
///     let _ = value.client_action();
///     let _ = value.dispatch();
///     let _ = value.process_handle();
/// }
/// ```
pub struct CheckedUntrustedMtgoDirectVisibleModelSelectionV1 {
    exact_producer_result_sha256: String,
    model_input_commitment_sha256: String,
    deployment_commitment_sha256: String,
    selection_commitment_sha256: String,
    response: MtgoPlayerVisibleDuelScoreResponseV1,
    selected_index: usize,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    confirmed_decision: crate::MtgoPlayerVisibleConfirmedDuelDecisionV1,
}

impl CheckedUntrustedMtgoDirectVisibleModelSelectionV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn selected_logit_f32_bits_v1(&self) -> u32 {
        self.response.ordered_action_logits_f32_bits[self.selected_index]
    }

    pub fn value_f32_bits_v1(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        &self.selection_commitment_sha256
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

    #[allow(dead_code)] // Reserved for the private dispatch transaction owner.
    pub(crate) fn exact_producer_result_sha256_v1(&self) -> &str {
        &self.exact_producer_result_sha256
    }

    #[allow(dead_code)] // Reserved for the private dispatch transaction owner.
    pub(crate) fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.model_input_commitment_sha256
    }

    #[allow(dead_code)] // Reserved for the private dispatch transaction owner.
    pub(crate) fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.deployment_commitment_sha256
    }
}

/// One selected action whose exact producer bytes were observed again after
/// scoring and remained byte-identical. The live producer independently
/// rebuilds and hashes the decision once more on MTGO's UI thread before its
/// private client-action call. This outer refresh closes selection drift but
/// still grants no input, event-entry, or spending authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1;
/// fn cannot_dispatch(value: &CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1) {
///     let _ = value.dispatch();
///     let _ = value.client_action();
///     let _ = value.process_handle();
/// }
/// ```
pub struct CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1 {
    exact_producer_result_sha256: String,
    model_input_commitment_sha256: String,
    deployment_commitment_sha256: String,
    selected_index: usize,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    confirmed_decision: crate::MtgoPlayerVisibleConfirmedDuelDecisionV1,
    selection_commitment_sha256: String,
    refresh_commitment_sha256: String,
}

impl CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn player_visible_confirmed_decision_v1(
        &self,
    ) -> &crate::MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        &self.confirmed_decision
    }

    pub fn refresh_commitment_sha256_v1(&self) -> &str {
        &self.refresh_commitment_sha256
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

    #[allow(dead_code)] // Reserved for the private authorized dispatch owner.
    pub(crate) fn exact_producer_result_sha256_v1(&self) -> &str {
        &self.exact_producer_result_sha256
    }

    pub(crate) fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.model_input_commitment_sha256
    }

    pub(crate) fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.deployment_commitment_sha256
    }
}

/// Requires a second exact producer observation after scoring. Any abstention,
/// byte change, action-order change, or visible-state change rejects. The
/// returned value is coordinate-free and non-authorizing.
pub fn refresh_direct_visible_selection_before_dispatch_v1(
    selection: CheckedUntrustedMtgoDirectVisibleModelSelectionV1,
    refreshed_exact_producer_result: &[u8],
) -> Result<CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1, MtgoContractErrorV1> {
    let MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { decision } =
        parse_and_validate_visible_duel_producer_result_v1(refreshed_exact_producer_result)?
    else {
        return Err(error_v1(
            "direct_visible_duel_refresh_abstained",
            "the refreshed producer result is not a complete visible decision",
        ));
    };
    let refreshed_sha256 = sha256_v1(refreshed_exact_producer_result);
    if refreshed_sha256 != selection.exact_producer_result_sha256 {
        return Err(error_v1(
            "direct_visible_duel_refresh_changed",
            "the exact producer result changed after model selection",
        ));
    }
    if decision.ordered_legal_actions.get(selection.selected_index)
        != Some(&selection.selected_action)
    {
        return Err(error_v1(
            "direct_visible_duel_refresh_selection_mismatch",
            "the refreshed action order does not retain the selected visible action",
        ));
    }
    let refresh_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_REFRESH_DOMAIN_V1,
        &[
            refreshed_sha256.as_bytes(),
            selection.selection_commitment_sha256.as_bytes(),
            &(selection.selected_index as u64).to_be_bytes(),
            b"second_exact_visible_observation_matches_no_input_authority",
        ],
    );
    Ok(CheckedUntrustedMtgoRefreshedDirectVisibleSelectionV1 {
        exact_producer_result_sha256: refreshed_sha256,
        model_input_commitment_sha256: selection.model_input_commitment_sha256,
        deployment_commitment_sha256: selection.deployment_commitment_sha256,
        selected_index: selection.selected_index,
        selected_action: selection.selected_action,
        confirmed_decision: selection.confirmed_decision,
        selection_commitment_sha256: selection.selection_commitment_sha256,
        refresh_commitment_sha256,
    })
}

/// Strictly parses one exact producer result and, only for a complete visible
/// decision, invokes the player-visible-only scorer. Selection is the greatest
/// finite logit; ties retain the earliest exact producer action index.
pub fn score_and_select_strict_visible_duel_producer_result_v1<S: MtgoPlayerVisibleDuelScorerV1>(
    exact_producer_result: &[u8],
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1, MtgoContractErrorV1> {
    require_sha256_v1(deployment_commitment_sha256)?;
    let exact_producer_result_sha256 = sha256_v1(exact_producer_result);
    match parse_and_validate_visible_duel_producer_result_v1(exact_producer_result)? {
        MtgoVisibleDuelViewModelBrokerResultV1::Abstained { reason } => {
            Ok(CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Abstained { reason })
        }
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision { decision } => {
            score_visible_decision_v1(
                *decision,
                exact_producer_result_sha256,
                deployment_commitment_sha256,
                scorer,
            )
            .map(Box::new)
            .map(CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected)
        }
    }
}

fn score_visible_decision_v1<S: MtgoPlayerVisibleDuelScorerV1>(
    decision: MtgoPlayerVisibleDuelDecisionInputV1,
    exact_producer_result_sha256: String,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoDirectVisibleModelSelectionV1, MtgoContractErrorV1> {
    let model_input_commitment_sha256 = decision.commitment_sha256_v1()?;
    let response = scorer
        .score_player_visible_duel_v1(&decision)
        .map_err(|error| error_v1("direct_visible_duel_scorer_failed", error))?;
    if response.schema_version != MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1
        || response.ordered_action_logits_f32_bits.len() != decision.ordered_legal_actions.len()
        || response.ordered_action_logits_f32_bits.is_empty()
    {
        return Err(error_v1(
            "direct_visible_duel_score_shape",
            "score must retain the exact nonempty producer action count",
        ));
    }
    let logits = response
        .ordered_action_logits_f32_bits
        .iter()
        .copied()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    if logits.iter().any(|value| !value.is_finite())
        || !f32::from_bits(response.value_f32_bits).is_finite()
    {
        return Err(error_v1(
            "direct_visible_duel_score_nonfinite",
            "every score output must be finite",
        ));
    }
    let mut selected_index = 0_usize;
    for index in 1..logits.len() {
        if logits[index].total_cmp(&logits[selected_index]).is_gt() {
            selected_index = index;
        }
    }
    let selected_action = decision.ordered_legal_actions[selected_index].clone();
    let confirmed_decision = crate::MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        current_state: decision.current_state.clone(),
        selected_action: selected_action.clone(),
    };
    let response_json = serde_json::to_vec(&response).map_err(|error| {
        error_v1(
            "direct_visible_duel_response_serialization",
            error.to_string(),
        )
    })?;
    let selected_action_json = serde_json::to_vec(&selected_action).map_err(|error| {
        error_v1(
            "direct_visible_duel_action_serialization",
            error.to_string(),
        )
    })?;
    let selection_commitment_sha256 = commitment_v1(
        DIRECT_VISIBLE_SELECTION_DOMAIN_V1,
        &[
            exact_producer_result_sha256.as_bytes(),
            model_input_commitment_sha256.as_bytes(),
            deployment_commitment_sha256.as_bytes(),
            &response_json,
            &(selected_index as u64).to_be_bytes(),
            &selected_action_json,
            b"strict_visible_decision_selected_no_input_authority",
        ],
    );
    Ok(CheckedUntrustedMtgoDirectVisibleModelSelectionV1 {
        exact_producer_result_sha256,
        model_input_commitment_sha256,
        deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
        selection_commitment_sha256,
        response,
        selected_index,
        selected_action,
        confirmed_decision,
    })
}

fn require_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "direct_visible_duel_deployment_hash",
            "deployment commitment must be lowercase SHA-256",
        ));
    }
    Ok(())
}

fn sha256_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleDuelStateV1,
        MtgoPlayerVisibleNamedCardV1, MtgoPlayerVisibleObjectRefV1, ZoneIndependentStepV1,
    };

    struct RecordingVisibleScorerV1 {
        called: bool,
        selected_index: usize,
        wrong_width: bool,
        saw_forbidden_field: bool,
    }

    impl MtgoPlayerVisibleDuelScorerV1 for RecordingVisibleScorerV1 {
        fn score_player_visible_duel_v1(
            &mut self,
            model_input: &MtgoPlayerVisibleDuelDecisionInputV1,
        ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
            self.called = true;
            let json = serde_json::to_string(model_input).unwrap();
            self.saw_forbidden_field = [
                "process_id",
                "client_object_id",
                "arena_id",
                "card_db_id",
                "frame_id",
            ]
            .iter()
            .any(|marker| json.contains(marker));
            let mut logits = vec![0.0_f32.to_bits(); model_input.ordered_legal_actions.len()];
            logits[self.selected_index] = 1.0_f32.to_bits();
            if self.wrong_width {
                logits.pop();
            }
            Ok(MtgoPlayerVisibleDuelScoreResponseV1 {
                schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
                ordered_action_logits_f32_bits: logits,
                value_f32_bits: 0.5_f32.to_bits(),
            })
        }
    }

    fn visible_result_v1() -> Vec<u8> {
        let object_ref = MtgoPlayerVisibleObjectRefV1 { visible_ordinal: 0 };
        let decision = MtgoPlayerVisibleDuelDecisionInputV1 {
            current_state: MtgoPlayerVisibleDuelStateV1 {
                acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 1,
                phase: ZoneIndependentStepV1::Main1,
                active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6]; 2],
                hand_counts: [1, 0],
                library_counts: [59, 60],
                battlefield: [Vec::new(), Vec::new()],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: false,
                    blockers_declared: false,
                    ordered_attackers: Vec::new(),
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: vec![MtgoPlayerVisibleNamedCardV1 {
                    object_ref,
                    card_name: "Plains".to_owned(),
                }],
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_legal_actions: vec![
                MtgoPlayerVisibleDuelActionV1::Pass {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                },
                MtgoPlayerVisibleDuelActionV1::PlayLand {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    source: object_ref,
                },
            ],
        };
        serde_json::to_vec(&MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision {
            decision: Box::new(decision),
        })
        .unwrap()
    }

    #[test]
    fn exact_visible_bytes_score_and_bind_the_selected_index() {
        let bytes = visible_result_v1();
        let mut scorer = RecordingVisibleScorerV1 {
            called: false,
            selected_index: 1,
            wrong_width: false,
            saw_forbidden_field: false,
        };
        let outcome = score_and_select_strict_visible_duel_producer_result_v1(
            &bytes,
            &"a".repeat(64),
            &mut scorer,
        )
        .unwrap();
        let CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(selection) = outcome else {
            panic!("complete decision unexpectedly abstained");
        };
        assert!(scorer.called);
        assert!(!scorer.saw_forbidden_field);
        assert_eq!(selection.selected_index_v1(), 1);
        assert!(matches!(
            selection.selected_action_v1(),
            MtgoPlayerVisibleDuelActionV1::PlayLand { .. }
        ));
        assert_eq!(
            selection.exact_producer_result_sha256_v1(),
            sha256_v1(&bytes)
        );
        assert_eq!(selection.selected_logit_f32_bits_v1(), 1.0_f32.to_bits());
        assert_eq!(selection.value_f32_bits_v1(), 0.5_f32.to_bits());
        assert!(!selection.safe_for_live_input_v1());
        assert!(!selection.permits_event_entry_v1());
        assert!(!selection.permits_spending_v1());
    }

    #[test]
    fn abstention_never_calls_the_scorer() {
        let bytes = br#"{"result_kind":"abstained","reason":"duel_surface_unavailable"}"#;
        let mut scorer = RecordingVisibleScorerV1 {
            called: false,
            selected_index: 0,
            wrong_width: false,
            saw_forbidden_field: false,
        };
        let outcome = score_and_select_strict_visible_duel_producer_result_v1(
            bytes,
            &"b".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert!(!scorer.called);
        assert!(matches!(
            outcome,
            CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Abstained {
                reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::DuelSurfaceUnavailable
            }
        ));
    }

    #[test]
    fn hidden_fields_bad_hash_and_wrong_score_width_reject() {
        let bytes = visible_result_v1();
        let mut scorer = RecordingVisibleScorerV1 {
            called: false,
            selected_index: 0,
            wrong_width: false,
            saw_forbidden_field: false,
        };
        assert!(score_and_select_strict_visible_duel_producer_result_v1(
            &bytes,
            "not-a-hash",
            &mut scorer,
        )
        .is_err());

        let mut hidden = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        hidden["decision"]["current_state"]["opponent_hidden_hand"] = serde_json::json!(["secret"]);
        assert!(score_and_select_strict_visible_duel_producer_result_v1(
            &serde_json::to_vec(&hidden).unwrap(),
            &"c".repeat(64),
            &mut scorer,
        )
        .is_err());

        let mut wrong = RecordingVisibleScorerV1 {
            called: false,
            selected_index: 0,
            wrong_width: true,
            saw_forbidden_field: false,
        };
        assert!(score_and_select_strict_visible_duel_producer_result_v1(
            &bytes,
            &"d".repeat(64),
            &mut wrong,
        )
        .is_err());
    }

    #[test]
    fn exact_byte_changes_change_the_private_binding() {
        let bytes = visible_result_v1();
        let mut spaced = Vec::with_capacity(bytes.len() + 1);
        spaced.push(b' ');
        spaced.extend_from_slice(&bytes);
        let mut first_scorer = RecordingVisibleScorerV1 {
            called: false,
            selected_index: 0,
            wrong_width: false,
            saw_forbidden_field: false,
        };
        let mut second_scorer = RecordingVisibleScorerV1 {
            called: false,
            selected_index: 0,
            wrong_width: false,
            saw_forbidden_field: false,
        };
        let CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(first) =
            score_and_select_strict_visible_duel_producer_result_v1(
                &bytes,
                &"e".repeat(64),
                &mut first_scorer,
            )
            .unwrap()
        else {
            panic!("first decision unexpectedly abstained");
        };
        let CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(second) =
            score_and_select_strict_visible_duel_producer_result_v1(
                &spaced,
                &"e".repeat(64),
                &mut second_scorer,
            )
            .unwrap()
        else {
            panic!("second decision unexpectedly abstained");
        };
        assert_eq!(
            first.model_input_commitment_sha256_v1(),
            second.model_input_commitment_sha256_v1()
        );
        assert_ne!(
            first.exact_producer_result_sha256_v1(),
            second.exact_producer_result_sha256_v1()
        );
        assert_ne!(
            first.selection_commitment_sha256_v1(),
            second.selection_commitment_sha256_v1()
        );
        assert_eq!(
            first.deployment_commitment_sha256_v1(),
            second.deployment_commitment_sha256_v1()
        );
    }

    #[test]
    fn second_exact_observation_preserves_only_the_selected_visible_action() {
        let bytes = visible_result_v1();
        let mut scorer = RecordingVisibleScorerV1 {
            called: false,
            selected_index: 1,
            wrong_width: false,
            saw_forbidden_field: false,
        };
        let CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(selection) =
            score_and_select_strict_visible_duel_producer_result_v1(
                &bytes,
                &"f".repeat(64),
                &mut scorer,
            )
            .unwrap()
        else {
            panic!("complete decision unexpectedly abstained");
        };
        let refreshed =
            refresh_direct_visible_selection_before_dispatch_v1(*selection, &bytes).unwrap();
        assert_eq!(refreshed.selected_index_v1(), 1);
        assert!(matches!(
            refreshed.selected_action_v1(),
            MtgoPlayerVisibleDuelActionV1::PlayLand { .. }
        ));
        assert_eq!(
            refreshed.exact_producer_result_sha256_v1(),
            sha256_v1(&bytes)
        );
        assert!(!refreshed.safe_for_live_input_v1());
        assert!(!refreshed.permits_event_entry_v1());
        assert!(!refreshed.permits_spending_v1());
    }

    #[test]
    fn changed_state_action_order_or_abstention_cannot_refresh_a_selection() {
        fn selection_v1(bytes: &[u8]) -> CheckedUntrustedMtgoDirectVisibleModelSelectionV1 {
            let mut scorer = RecordingVisibleScorerV1 {
                called: false,
                selected_index: 1,
                wrong_width: false,
                saw_forbidden_field: false,
            };
            let CheckedUntrustedMtgoDirectVisibleScoringOutcomeV1::Selected(selection) =
                score_and_select_strict_visible_duel_producer_result_v1(
                    bytes,
                    &"1".repeat(64),
                    &mut scorer,
                )
                .unwrap()
            else {
                panic!("complete decision unexpectedly abstained");
            };
            *selection
        }

        let bytes = visible_result_v1();
        let mut changed_state = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        changed_state["decision"]["current_state"]["life_totals"][0] = serde_json::json!(19);
        let changed_state = serde_json::to_vec(&changed_state).unwrap();
        assert_eq!(
            refresh_direct_visible_selection_before_dispatch_v1(
                selection_v1(&bytes),
                &changed_state,
            )
            .err()
            .unwrap()
            .code(),
            "direct_visible_duel_refresh_changed"
        );

        let mut reordered = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        reordered["decision"]["ordered_legal_actions"]
            .as_array_mut()
            .unwrap()
            .reverse();
        let reordered = serde_json::to_vec(&reordered).unwrap();
        assert_eq!(
            refresh_direct_visible_selection_before_dispatch_v1(selection_v1(&bytes), &reordered)
                .err()
                .unwrap()
                .code(),
            "direct_visible_duel_refresh_changed"
        );

        let abstained = br#"{"result_kind":"abstained","reason":"projection_incomplete"}"#;
        assert_eq!(
            refresh_direct_visible_selection_before_dispatch_v1(selection_v1(&bytes), abstained)
                .err()
                .unwrap()
                .code(),
            "direct_visible_duel_refresh_abstained"
        );
    }
}
