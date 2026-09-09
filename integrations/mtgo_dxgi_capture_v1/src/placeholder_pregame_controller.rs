//! Placeholder pregame controller (spec 6.5, provisional). Deterministic
//! mulligan and London-bottoming rules for the seated player's own hand. It
//! is a disclosed non-model component; Jack's ratified rule table replaces
//! it behind the same slot. It never grants live authority.

use crate::{
    MtgoCompetitiveNativePregameActionV1, MtgoCompetitiveNativePregameModelInputV1,
    MtgoCompetitiveNativePregameScoreResponseV1, MtgoCompetitiveNativePregameScorerV1,
    MtgoCompetitiveNativeSideboardConfigurationV1, MtgoCompetitivePregameStageV1,
    MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1,
    MtgoResolvedVisibleCardV1, MtgoUnknownCardPolicyV1,
    MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The provisional rule table, spelled out so its commitment is auditable.
pub const MTGO_PLACEHOLDER_PREGAME_RULE_TABLE_V1: &str = "keep_seven_with_two_to_five_lands;\
mulligan_once_to_six_otherwise;\
never_below_six;\
bottom_excess_land_highest_slot_when_lands_exceed_five;\
bottom_highest_mana_value_nonland_ties_to_highest_slot;\
bottom_highest_slot_when_no_nonland_remains;\
submit_when_selected_equals_required";

pub struct MtgoPlaceholderPregameControllerV1 {
    card_facts: BTreeMap<String, MtgoResolvedVisibleCardV1>,
    rule_table_commitment_sha256: String,
}

impl MtgoPlaceholderPregameControllerV1 {
    /// Resolves every deck card once at construction so a deck outside the
    /// kernel catalog fails before any decision is scored.
    pub fn new_v1(
        deck: &MtgoCompetitiveNativeSideboardConfigurationV1,
        policy: MtgoUnknownCardPolicyV1,
    ) -> Result<Self, String> {
        let names: Vec<&str> = deck
            .mainboard
            .iter()
            .chain(deck.sideboard.iter())
            .map(|card| card.visible_card_name.as_str())
            .collect();
        let resolved = policy
            .resolve_visible_card_names_v1(&names)
            .map_err(|abstention| {
                format!(
                    "{}:{}",
                    abstention.reason,
                    abstention.unknown_visible_card_names.join(",")
                )
            })?;
        let card_facts = resolved
            .into_iter()
            .map(|card| (card.visible_card_name.clone(), card))
            .collect();
        let mut hasher = Sha256::new();
        hasher.update(b"mtgo-placeholder-pregame-rule-table-v1\0");
        hasher.update(MTGO_PLACEHOLDER_PREGAME_RULE_TABLE_V1.as_bytes());
        let rule_table_commitment_sha256 = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            card_facts,
            rule_table_commitment_sha256,
        })
    }

    pub fn rule_table_commitment_sha256_v1(&self) -> &str {
        &self.rule_table_commitment_sha256
    }

    fn facts_v1(&self, visible_card_name: &str) -> Result<&MtgoResolvedVisibleCardV1, String> {
        self.card_facts
            .get(visible_card_name)
            .ok_or_else(|| format!("pregame_visible_card_outside_deck:{visible_card_name}"))
    }

    fn action_index_v1(
        input: &MtgoCompetitiveNativePregameModelInputV1,
        wanted: &MtgoCompetitiveNativePregameActionV1,
    ) -> Option<usize> {
        input
            .ordered_actions
            .iter()
            .position(|action| action == wanted)
    }

    /// Chooses the index into `ordered_actions` the rule table selects.
    pub fn choose_index_v1(
        &self,
        input: &MtgoCompetitiveNativePregameModelInputV1,
    ) -> Result<usize, String> {
        let mut land_count = 0_usize;
        for card in &input.ordered_visible_cards {
            if self.facts_v1(&card.visible_card_name)?.is_land && !card.selected_for_bottom {
                land_count += 1;
            }
        }
        match input.stage {
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size,
            } => {
                let keep = Self::action_index_v1(
                    input,
                    &MtgoCompetitiveNativePregameActionV1::KeepOpeningHand,
                )
                .ok_or_else(|| {
                    "placeholder pregame controller requires a keep action".to_owned()
                })?;
                if prospective_keep_size <= 6 || (2..=5).contains(&land_count) {
                    return Ok(keep);
                }
                Ok(Self::action_index_v1(
                    input,
                    &MtgoCompetitiveNativePregameActionV1::Mulligan { next_hand_size: 6 },
                )
                .unwrap_or(keep))
            }
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count,
            } => {
                if selected_bottom_count >= required_bottom_count {
                    return Self::action_index_v1(
                        input,
                        &MtgoCompetitiveNativePregameActionV1::SubmitBottoming,
                    )
                    .ok_or_else(|| {
                        "placeholder pregame controller requires a submit action".to_owned()
                    });
                }
                let mut candidates: Vec<(u8, usize, bool, u8)> = Vec::new();
                for (index, action) in input.ordered_actions.iter().enumerate() {
                    if let MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot } =
                        action
                    {
                        let card = input
                            .ordered_visible_cards
                            .get(usize::from(*card_slot))
                            .ok_or_else(|| {
                                "placeholder pregame controller saw an unknown slot".to_owned()
                            })?;
                        let facts = self.facts_v1(&card.visible_card_name)?;
                        candidates.push((*card_slot, index, facts.is_land, facts.mana_value));
                    }
                }
                if candidates.is_empty() {
                    return Err(
                        "placeholder pregame controller has no bottoming candidate".to_owned()
                    );
                }
                let chosen = if land_count > 5 {
                    candidates
                        .iter()
                        .filter(|candidate| candidate.2)
                        .max_by_key(|candidate| candidate.0)
                } else {
                    candidates
                        .iter()
                        .filter(|candidate| !candidate.2)
                        .max_by_key(|candidate| (candidate.3, candidate.0))
                };
                let chosen = chosen
                    .or_else(|| candidates.iter().max_by_key(|candidate| candidate.0))
                    .ok_or_else(|| {
                        "placeholder pregame controller has no bottoming candidate".to_owned()
                    })?;
                Ok(chosen.1)
            }
            MtgoCompetitivePregameStageV1::GameplayReady => {
                Err("placeholder pregame controller has no gameplay-ready decision".to_owned())
            }
        }
    }
}

impl MtgoCompetitiveNativePregameScorerV1 for MtgoPlaceholderPregameControllerV1 {
    fn score_pregame_v1(
        &mut self,
        model_input: &MtgoCompetitiveNativePregameModelInputV1,
        model_input_commitment_sha256: &str,
        deployment_commitment_sha256: &str,
    ) -> Result<MtgoCompetitiveNativePregameScoreResponseV1, String> {
        let chosen = self.choose_index_v1(model_input)?;
        let ordered_action_logits_f32_bits = (0..model_input.ordered_actions.len())
            .map(|index| if index == chosen { 1.0_f32 } else { 0.0_f32 }.to_bits())
            .collect();
        Ok(MtgoCompetitiveNativePregameScoreResponseV1 {
            schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
            model_input_commitment_sha256: model_input_commitment_sha256.to_owned(),
            deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
            ordered_action_logits_f32_bits,
            value_f32_bits: 0.0_f32.to_bits(),
        })
    }
}

impl MtgoDeploymentSlotV1 for MtgoPlaceholderPregameControllerV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: format!(
                "placeholder_pregame_controller_rule_table_v1:{}",
                self.rule_table_commitment_sha256
            ),
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
        score_checked_untrusted_competitive_native_pregame_v1, MtgoCompetitiveNativePregameCardV1,
        MtgoCompetitiveNativeSideboardCardCountV1, MtgoCompetitiveNativeSideboardConfigurationV1,
        MtgoCompetitivePregameStageV1,
    };
    use mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1;

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

    fn hand(names: [&str; 7], selected: &[u8]) -> Vec<MtgoCompetitiveNativePregameCardV1> {
        names
            .iter()
            .enumerate()
            .map(|(slot, name)| MtgoCompetitiveNativePregameCardV1 {
                card_slot: slot as u8,
                visible_card_name: (*name).to_owned(),
                selected_for_bottom: selected.contains(&(slot as u8)),
            })
            .collect()
    }

    fn mulligan_input(
        names: [&str; 7],
        prospective_keep_size: u8,
    ) -> MtgoCompetitiveNativePregameModelInputV1 {
        MtgoCompetitiveNativePregameModelInputV1 {
            game_number: 1,
            play_draw: MtgoCompetitivePregamePlayDrawV1::OnPlay,
            acting_player_games_won: 0,
            opponent_games_won: 0,
            player_known_deck_configuration: deck_v1(),
            stage: MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size,
            },
            prospective_keep_size: Some(prospective_keep_size),
            required_bottom_count: 0,
            selected_bottom_count: 0,
            ordered_visible_cards: hand(names, &[]),
            ordered_confirmed_bottom_slots: Vec::new(),
            ordered_actions: vec![
                MtgoCompetitiveNativePregameActionV1::KeepOpeningHand,
                MtgoCompetitiveNativePregameActionV1::Mulligan {
                    next_hand_size: prospective_keep_size - 1,
                },
            ],
        }
    }

    fn bottoming_input(
        names: [&str; 7],
        selected: &[u8],
        required: u8,
    ) -> MtgoCompetitiveNativePregameModelInputV1 {
        let cards = hand(names, selected);
        let mut actions: Vec<MtgoCompetitiveNativePregameActionV1> = cards
            .iter()
            .filter(|card| !card.selected_for_bottom)
            .map(
                |card| MtgoCompetitiveNativePregameActionV1::SelectForBottom {
                    card_slot: card.card_slot,
                },
            )
            .collect();
        actions.push(MtgoCompetitiveNativePregameActionV1::SubmitBottoming);
        MtgoCompetitiveNativePregameModelInputV1 {
            game_number: 1,
            play_draw: MtgoCompetitivePregamePlayDrawV1::OnPlay,
            acting_player_games_won: 0,
            opponent_games_won: 0,
            player_known_deck_configuration: deck_v1(),
            stage: MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: required,
                selected_bottom_count: selected.len() as u8,
            },
            prospective_keep_size: Some(7 - required),
            required_bottom_count: required,
            selected_bottom_count: selected.len() as u8,
            ordered_visible_cards: cards,
            ordered_confirmed_bottom_slots: selected.to_vec(),
            ordered_actions: actions,
        }
    }

    fn controller() -> MtgoPlaceholderPregameControllerV1 {
        MtgoPlaceholderPregameControllerV1::new_v1(
            &deck_v1(),
            MtgoUnknownCardPolicyV1::FailClosedHumanTakeover,
        )
        .unwrap()
    }

    const M: &str = "Mountain";
    const B: &str = "Lightning Bolt";

    #[test]
    fn keeps_seven_with_two_to_five_lands() {
        let input = mulligan_input([M, M, M, B, B, B, B], 7);
        assert_eq!(controller().choose_index_v1(&input).unwrap(), 0);
    }

    #[test]
    fn mulligans_seven_with_one_land_and_with_six_lands() {
        assert_eq!(
            controller()
                .choose_index_v1(&mulligan_input([M, B, B, B, B, B, B], 7))
                .unwrap(),
            1
        );
        assert_eq!(
            controller()
                .choose_index_v1(&mulligan_input([M, M, M, M, M, M, B], 7))
                .unwrap(),
            1
        );
    }

    #[test]
    fn never_goes_below_six() {
        let input = mulligan_input([B, B, B, B, B, B, B], 6);
        assert_eq!(controller().choose_index_v1(&input).unwrap(), 0);
    }

    #[test]
    fn bottoms_the_highest_mana_value_nonland_then_submits() {
        let first = bottoming_input([M, M, M, B, B, B, B], &[], 1);
        // Non-lands tie at mana value 1, so the highest slot (6) is bottomed.
        let chosen = controller().choose_index_v1(&first).unwrap();
        assert_eq!(
            first.ordered_actions[chosen],
            MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot: 6 }
        );
        let second = bottoming_input([M, M, M, B, B, B, B], &[6], 1);
        let chosen = controller().choose_index_v1(&second).unwrap();
        assert_eq!(
            second.ordered_actions[chosen],
            MtgoCompetitiveNativePregameActionV1::SubmitBottoming
        );
    }

    #[test]
    fn bottoms_an_excess_land_when_more_than_five_lands_remain() {
        let input = bottoming_input([M, M, M, M, M, M, B], &[], 1);
        let chosen = controller().choose_index_v1(&input).unwrap();
        assert_eq!(
            input.ordered_actions[chosen],
            MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot: 5 }
        );
    }

    #[test]
    fn hand_card_outside_the_deck_fails_closed() {
        let input = mulligan_input([M, M, "Island", B, B, B, B], 7);
        assert!(controller()
            .choose_index_v1(&input)
            .unwrap_err()
            .contains("pregame_visible_card_outside_deck"));
    }

    #[test]
    fn unknown_deck_card_fails_construction() {
        let mut deck = deck_v1();
        deck.mainboard[0].visible_card_name = "Not A Kernel Card".to_owned();
        assert!(MtgoPlaceholderPregameControllerV1::new_v1(
            &deck,
            MtgoUnknownCardPolicyV1::FailClosedHumanTakeover
        )
        .is_err());
    }

    #[test]
    fn scores_through_the_checked_untrusted_pregame_path() {
        let input = mulligan_input([M, M, M, B, B, B, B], 7);
        let mut scorer = controller();
        let selection = score_checked_untrusted_competitive_native_pregame_v1(
            &input,
            &"c".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert_eq!(selection.selected_index_v1(), 0);
        assert_eq!(
            selection.selected_action_v1(),
            &MtgoCompetitiveNativePregameActionV1::KeepOpeningHand
        );
        assert!(!selection.safe_for_live_input_v1());
        assert!(!selection.permits_event_entry_v1());
        assert!(!selection.permits_spending_v1());
    }

    #[test]
    fn slot_descriptor_is_a_placeholder_bound_to_the_rule_table_commitment() {
        let controller = controller();
        let descriptor = controller.slot_descriptor_v1(MtgoDeploymentSlotKindV1::PregameController);
        assert!(descriptor.is_placeholder);
        assert!(!descriptor.qualified_for_live);
        assert!(descriptor
            .implementation_id
            .starts_with("placeholder_pregame_controller_rule_table_v1:"));
        assert!(descriptor
            .implementation_id
            .ends_with(controller.rule_table_commitment_sha256_v1()));
    }
}
