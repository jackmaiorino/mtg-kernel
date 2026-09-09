//! Placeholder sideboard controller (spec 6.5): submits the current
//! configuration unchanged before every game after the first, on both the
//! whole-target and the sequential deliberation interfaces. A trained
//! sideboard head replaces it behind the same slot.

use crate::{
    MtgoCompetitiveNativeSideboardDeliberationActionV1,
    MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
    MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1,
    MtgoCompetitiveNativeSideboardDeliberationScorerV1, MtgoCompetitiveNativeSideboardModelInputV1,
    MtgoCompetitiveNativeSideboardModelSelectionV1, MtgoCompetitiveNativeSideboardScoreResponseV1,
    MtgoCompetitiveNativeSideboardScorerV1, MtgoDeploymentSlotDescriptorV1,
    MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1,
    MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct MtgoPlaceholderSideboardControllerV1;

impl MtgoCompetitiveNativeSideboardScorerV1 for MtgoPlaceholderSideboardControllerV1 {
    fn score_sideboard_v1(
        &mut self,
        model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
        model_input_commitment_sha256: &str,
        deployment_commitment_sha256: &str,
    ) -> Result<MtgoCompetitiveNativeSideboardScoreResponseV1, String> {
        Ok(MtgoCompetitiveNativeSideboardScoreResponseV1 {
            schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
            model_input_commitment_sha256: model_input_commitment_sha256.to_owned(),
            deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
            selection: MtgoCompetitiveNativeSideboardModelSelectionV1 {
                target_configuration: model_input.current_configuration.clone(),
            },
            value_f32_bits: 0.0_f32.to_bits(),
        })
    }
}

impl MtgoCompetitiveNativeSideboardDeliberationScorerV1 for MtgoPlaceholderSideboardControllerV1 {
    fn score_sideboard_deliberation_v1(
        &mut self,
        decision: &MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
    ) -> Result<MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1, String> {
        let submit = decision
            .ordered_actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration
                )
            })
            .ok_or_else(|| {
                "placeholder sideboard controller requires a submit action".to_owned()
            })?;
        Ok(MtgoCompetitiveNativeSideboardDeliberationScoreResponseV1 {
            schema_version: decision.schema_version,
            decision_commitment_sha256: decision.decision_commitment_sha256.clone(),
            deployment_commitment_sha256: decision.deployment_commitment_sha256.clone(),
            ordered_logits_f32_bits: (0..decision.ordered_actions.len())
                .map(|index| if index == submit { 1.0_f32 } else { 0.0_f32 }.to_bits())
                .collect(),
            value_f32_bits: 0.0_f32.to_bits(),
        })
    }
}

impl MtgoDeploymentSlotV1 for MtgoPlaceholderSideboardControllerV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: "placeholder_sideboard_controller_unchanged_v1".to_owned(),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        advance_competitive_native_sideboard_deliberation_v1,
        begin_competitive_native_sideboard_deliberation_v1,
        score_checked_untrusted_competitive_native_sideboard_v1,
        MtgoCompetitiveNativeSideboardCardCountV1, MtgoCompetitiveNativeSideboardConfigurationV1,
        MtgoCompetitiveNativeSideboardDeliberationActionV1,
        MtgoCompetitiveNativeSideboardDeliberationAdvanceV1,
        MtgoCompetitiveNativeSideboardDeliberationDecisionV1,
        MtgoCompetitiveNativeSideboardModelInputV1,
        MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
    };

    fn deck_v1() -> MtgoCompetitiveNativeSideboardConfigurationV1 {
        MtgoCompetitiveNativeSideboardConfigurationV1 {
            mainboard: vec![
                MtgoCompetitiveNativeSideboardCardCountV1 {
                    visible_card_name: "Lightning Bolt".to_owned(),
                    count: 4,
                },
                MtgoCompetitiveNativeSideboardCardCountV1 {
                    visible_card_name: "Mountain".to_owned(),
                    count: 56,
                },
            ],
            sideboard: vec![MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Lightning Bolt".to_owned(),
                count: 15,
            }],
        }
    }

    fn input_v1() -> MtgoCompetitiveNativeSideboardModelInputV1 {
        MtgoCompetitiveNativeSideboardModelInputV1 {
            next_game_number: 2,
            acting_player_games_won: 1,
            opponent_games_won: 0,
            current_configuration: deck_v1(),
        }
    }

    #[test]
    fn whole_target_response_is_the_unchanged_current_configuration() {
        let mut controller = MtgoPlaceholderSideboardControllerV1;
        let selection = score_checked_untrusted_competitive_native_sideboard_v1(
            &input_v1(),
            &"d".repeat(64),
            &mut controller,
        )
        .unwrap();
        assert_eq!(selection.selection_v1().target_configuration, deck_v1());
        assert!(selection.no_changes_selected_v1(&input_v1()));
        assert!(!selection.safe_for_live_input_v1());
        assert!(!selection.permits_sideboard_submission_v1());
        assert!(!selection.permits_spending_v1());
    }

    #[test]
    fn deliberation_selects_submit_configuration_and_rejects_its_absence() {
        let mut controller = MtgoPlaceholderSideboardControllerV1;
        let decision = MtgoCompetitiveNativeSideboardDeliberationDecisionV1 {
            schema_version: MTGO_COMPETITIVE_NATIVE_SIDEBOARD_DELIBERATION_SCHEMA_V1,
            decision_number: 1,
            model_input_commitment_sha256: "a".repeat(64),
            deployment_commitment_sha256: "b".repeat(64),
            prior_trace_commitment_sha256: "c".repeat(64),
            candidate_configuration: deck_v1(),
            ordered_actions: vec![
                MtgoCompetitiveNativeSideboardDeliberationActionV1::MoveOneToMainboard {
                    visible_card_name: "Lightning Bolt".to_owned(),
                },
                MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration,
            ],
            decision_commitment_sha256: "e".repeat(64),
        };
        let response = controller
            .score_sideboard_deliberation_v1(&decision)
            .unwrap();
        assert_eq!(
            response.ordered_logits_f32_bits,
            vec![0.0_f32.to_bits(), 1.0_f32.to_bits()]
        );
        assert_eq!(response.decision_commitment_sha256, "e".repeat(64));
        assert_eq!(response.deployment_commitment_sha256, "b".repeat(64));
        let mut without_submit = decision;
        without_submit.ordered_actions.pop();
        assert!(controller
            .score_sideboard_deliberation_v1(&without_submit)
            .is_err());
    }

    #[test]
    fn deliberation_driver_submits_the_unchanged_current_configuration() {
        let mut controller = MtgoPlaceholderSideboardControllerV1;
        let deliberation =
            begin_competitive_native_sideboard_deliberation_v1(input_v1(), &"b".repeat(64))
                .unwrap();
        let advance =
            advance_competitive_native_sideboard_deliberation_v1(deliberation, &mut controller)
                .unwrap();
        let (submission, receipt) = match advance {
            MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Submitted {
                submission,
                receipt,
            } => (submission, receipt),
            MtgoCompetitiveNativeSideboardDeliberationAdvanceV1::Continue { .. } => {
                panic!("placeholder sideboard controller did not submit on the first decision")
            }
        };
        assert_eq!(submission.decisions_consumed_v1(), 1);
        assert_eq!(receipt.decision_number, 1);
        assert_eq!(
            receipt.selected_action,
            MtgoCompetitiveNativeSideboardDeliberationActionV1::SubmitConfiguration
        );
        assert_eq!(submission.selection_v1().target_configuration, deck_v1());
    }

    #[test]
    fn slot_descriptor_is_a_placeholder() {
        let descriptor = MtgoPlaceholderSideboardControllerV1
            .slot_descriptor_v1(MtgoDeploymentSlotKindV1::SideboardController);
        assert_eq!(
            descriptor.implementation_id,
            "placeholder_sideboard_controller_unchanged_v1"
        );
        assert!(descriptor.is_placeholder);
        assert!(!descriptor.qualified_for_live);
    }
}
