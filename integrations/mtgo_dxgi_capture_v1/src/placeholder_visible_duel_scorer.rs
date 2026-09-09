//! Placeholder duel-scorer slot. It resolves every visible card name through
//! the unknown-card policy and then fails closed with a fixed reason, because
//! the kernel-owned player-visible scorer (spec 6.3) is not built yet. That
//! scorer replaces this placeholder behind the same slot.

use crate::{
    MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1,
    MtgoUnknownCardPolicyV1,
};
use mtgo_blackbox_v1::{
    MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelScoreResponseV1,
    MtgoPlayerVisibleDuelScorerV1,
};

pub const MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1: &str =
    "player_visible_kernel_scorer_not_qualified_v1";

/// Every card name the seated player can see in one decision, sorted and
/// deduplicated. Hidden zones are never enumerated because the input schema
/// cannot carry them.
pub fn visible_card_names_v1(input: &MtgoPlayerVisibleDuelDecisionInputV1) -> Vec<String> {
    let state = &input.current_state;
    let mut names: Vec<String> = Vec::new();
    names.extend(state.own_hand.iter().map(|card| card.card_name.clone()));
    for side in &state.battlefield {
        names.extend(side.iter().map(|card| card.card_name.clone()));
    }
    for side in &state.graveyards {
        names.extend(side.iter().map(|card| card.card_name.clone()));
    }
    names.extend(
        state
            .exile
            .iter()
            .filter_map(|card| card.visible_card_name.clone()),
    );
    names.extend(
        state
            .stack
            .iter()
            .filter_map(|item| item.visible_source_name.clone()),
    );
    for side in &state.known_library_cards {
        names.extend(side.iter().map(|card| card.card.card_name.clone()));
    }
    for side in &state.known_hand_cards {
        names.extend(side.iter().map(|card| card.card_name.clone()));
    }
    names.sort();
    names.dedup();
    names
}

pub struct MtgoPlaceholderVisibleDuelScorerV1 {
    policy: MtgoUnknownCardPolicyV1,
}

impl MtgoPlaceholderVisibleDuelScorerV1 {
    pub fn new_v1(policy: MtgoUnknownCardPolicyV1) -> Self {
        Self { policy }
    }
}

impl MtgoPlayerVisibleDuelScorerV1 for MtgoPlaceholderVisibleDuelScorerV1 {
    fn score_player_visible_duel_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleDuelDecisionInputV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
        let names = visible_card_names_v1(model_input);
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        self.policy
            .resolve_visible_card_names_v1(&borrowed)
            .map_err(|abstention| {
                format!(
                    "{}:{}",
                    abstention.reason,
                    abstention.unknown_visible_card_names.join(",")
                )
            })?;
        Err(MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1.to_owned())
    }
}

impl MtgoDeploymentSlotV1 for MtgoPlaceholderVisibleDuelScorerV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: "placeholder_visible_duel_scorer_fail_closed_v1".to_owned(),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1;
    use mtgo_blackbox_v1::{MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelScorerV1};

    fn sample_input() -> MtgoPlayerVisibleDuelDecisionInputV1 {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../mtgo_blackbox_v1/fixtures/player_visible_flat_v2_conformance_source_v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["model_input"].clone()).unwrap()
    }

    #[test]
    fn collects_every_visible_name_sorted_and_deduplicated() {
        let names = visible_card_names_v1(&sample_input());
        assert!(!names.is_empty());
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn all_known_names_fail_closed_with_the_not_qualified_reason() {
        let mut scorer = MtgoPlaceholderVisibleDuelScorerV1::new_v1(
            MtgoUnknownCardPolicyV1::FailClosedHumanTakeover,
        );
        let error = scorer
            .score_player_visible_duel_v1(&sample_input())
            .unwrap_err();
        assert_eq!(error, MTGO_VISIBLE_KERNEL_SCORER_NOT_QUALIFIED_REASON_V1);
    }

    #[test]
    fn an_unknown_name_abstains_before_the_not_qualified_reason() {
        let mut input = sample_input();
        input.current_state.battlefield[1].push(
            mtgo_blackbox_v1::MtgoPlayerVisibleBattlefieldCardV1 {
                object_ref: mtgo_blackbox_v1::MtgoPlayerVisibleObjectRefV1 {
                    visible_ordinal: 9_000,
                },
                card_name: "Not A Kernel Card".to_owned(),
                tapped: false,
                marked_damage: 0,
                counters: mtgo_blackbox_v1::MtgoPlayerVisibleCounterStateV1 {
                    plus_one_plus_one: 0,
                    minus_one_minus_one: 0,
                    minus_zero_minus_one: 0,
                    stun: 0,
                    lore: 0,
                },
                is_token: false,
                visible_effective_power: None,
                visible_effective_toughness: None,
            },
        );
        let mut scorer = MtgoPlaceholderVisibleDuelScorerV1::new_v1(
            MtgoUnknownCardPolicyV1::FailClosedHumanTakeover,
        );
        let error = scorer.score_player_visible_duel_v1(&input).unwrap_err();
        assert!(error.starts_with(MTGO_UNKNOWN_CARD_FAIL_CLOSED_REASON_V1));
        assert!(error.contains("Not A Kernel Card"));
    }

    #[test]
    fn placeholder_builder_wires_all_five_slots_as_placeholders() {
        let deck = crate::MtgoCompetitiveNativeSideboardConfigurationV1 {
            mainboard: vec![
                crate::MtgoCompetitiveNativeSideboardCardCountV1 {
                    visible_card_name: "Lightning Bolt".to_owned(),
                    count: 4,
                },
                crate::MtgoCompetitiveNativeSideboardCardCountV1 {
                    visible_card_name: "Mountain".to_owned(),
                    count: 56,
                },
            ],
            sideboard: vec![crate::MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Lightning Bolt".to_owned(),
                count: 15,
            }],
        };
        let slots = crate::build_placeholder_deployment_slots_v1(&deck).unwrap();
        let report = slots.slot_report_v1();
        assert!(report.all_slots_wired);
        assert_eq!(report.placeholder_slot_count, 5);
        assert!(!report.grants_live_authority);
    }
}
