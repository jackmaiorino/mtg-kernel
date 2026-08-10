use super::{
    canonical_json_commitment_v3, card_aware_bottoming_scoring_request_commitment_v5,
    card_aware_pregame_scoring_request_commitment_v4, model_deployment_commitment_v1,
    sha256_hex_v1, MtgoCardAwareBottomingScoreResponseV5, MtgoCardAwareBottomingScoringRequestV5,
    MtgoCardAwarePregameScoreResponseV4, MtgoCardAwarePregameScoringRequestV4,
    MtgoExpectedModelDeploymentV1, MtgoExternalCardAwareBottomingScorerV5,
    MtgoExternalCardAwarePregameScorerV4, MtgoPregameActionSemanticV1,
    MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5, MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4,
};
use mtgo_blackbox_v1::{
    MtgoNativeCheckpointIdentityV1, MtgoOfflineBottomingActionSemanticV1,
    MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MTGO_NON_MODEL_PREGAME_HEURISTIC_SCHEMA_V1: u32 = 1;

pub const MTGO_HEURISTIC_COLOR_WHITE_V1: u8 = 1 << 0;
pub const MTGO_HEURISTIC_COLOR_BLUE_V1: u8 = 1 << 1;
pub const MTGO_HEURISTIC_COLOR_BLACK_V1: u8 = 1 << 2;
pub const MTGO_HEURISTIC_COLOR_RED_V1: u8 = 1 << 3;
pub const MTGO_HEURISTIC_COLOR_GREEN_V1: u8 = 1 << 4;
pub const MTGO_HEURISTIC_COLORLESS_V1: u8 = 1 << 5;
const MTGO_HEURISTIC_ALL_COLORS_V1: u8 = MTGO_HEURISTIC_COLOR_WHITE_V1
    | MTGO_HEURISTIC_COLOR_BLUE_V1
    | MTGO_HEURISTIC_COLOR_BLACK_V1
    | MTGO_HEURISTIC_COLOR_RED_V1
    | MTGO_HEURISTIC_COLOR_GREEN_V1
    | MTGO_HEURISTIC_COLORLESS_V1;

const HEURISTIC_PROFILE_DOMAIN_V1: &[u8] = b"mtgo-non-model-pregame-heuristic-profile-v1";
const HEURISTIC_COMPATIBILITY_DIGEST_DOMAIN_V1: &[u8] =
    b"mtgo-non-model-pregame-heuristic-compatibility-digest-v1";
const FIXED_BASICS_PROFILE_ID_V1: &str = "fixed-30-plains-30-island-wiring-only-v1";
const KERNEL_BASICS_PROFILE_ID_V1: &str = "fixed-30-forest-30-island-wiring-only-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "card_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoHeuristicCardKindV1 {
    Land {
        produced_color_mask: u8,
    },
    Spell {
        mana_value: u8,
        required_color_mask: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoHeuristicCardFeatureV1 {
    pub visible_card_name: String,
    pub kind: MtgoHeuristicCardKindV1,
}

/// A caller-supplied, source-committed card feature table for the deliberately
/// non-model pregame stopgap. It is not a card database or a training artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoNonModelPregameHeuristicProfileV1 {
    pub schema_version: u32,
    pub profile_id: String,
    pub source_card_catalog_sha256: String,
    pub cards: Vec<MtgoHeuristicCardFeatureV1>,
}

/// Deterministic Keep, Mulligan, and London-bottom scorer used only because
/// the trained checkpoint lineage has no pregame representation.
///
/// The compatibility deployment is encoded in the historical v4/v5 request
/// field so every response still binds this exact profile. Its checkpoint
/// fields are domain-separated sentinels, not checkpoint claims. This type
/// receives no pixels, coordinates, authorization, or input capability.
#[derive(Clone)]
pub struct MtgoNonModelPregameHeuristicV1 {
    profile: MtgoNonModelPregameHeuristicProfileV1,
    profile_commitment_sha256: String,
    compatibility_deployment: MtgoExpectedModelDeploymentV1,
    cards_by_name: HashMap<String, MtgoHeuristicCardKindV1>,
}

impl MtgoNonModelPregameHeuristicV1 {
    pub fn new_v1(profile: MtgoNonModelPregameHeuristicProfileV1) -> Result<Self, String> {
        validate_heuristic_profile_v1(&profile)?;
        let profile_commitment_sha256 =
            non_model_pregame_heuristic_profile_commitment_v1(&profile)?;
        let compatibility_deployment =
            compatibility_deployment_v1(&profile, &profile_commitment_sha256)?;
        let cards_by_name = profile
            .cards
            .iter()
            .map(|card| (card.visible_card_name.clone(), card.kind.clone()))
            .collect();
        Ok(Self {
            profile,
            profile_commitment_sha256,
            compatibility_deployment,
            cards_by_name,
        })
    }

    /// The current 30 Plains, 30 Island calibration deck. This exists only to
    /// exercise the full pregame wiring and is not a competitive policy.
    pub fn fixed_basic_lands_wiring_only_v1() -> Result<Self, String> {
        Self::new_v1(MtgoNonModelPregameHeuristicProfileV1 {
            schema_version: MTGO_NON_MODEL_PREGAME_HEURISTIC_SCHEMA_V1,
            profile_id: FIXED_BASICS_PROFILE_ID_V1.to_owned(),
            source_card_catalog_sha256: sha256_hex_v1(
                b"mtgo-fixed-30-plains-30-island-visible-label-catalog-v1",
            ),
            cards: vec![
                MtgoHeuristicCardFeatureV1 {
                    visible_card_name: "Plains".to_owned(),
                    kind: MtgoHeuristicCardKindV1::Land {
                        produced_color_mask: MTGO_HEURISTIC_COLOR_WHITE_V1,
                    },
                },
                MtgoHeuristicCardFeatureV1 {
                    visible_card_name: "Island".to_owned(),
                    kind: MtgoHeuristicCardKindV1::Land {
                        produced_color_mask: MTGO_HEURISTIC_COLOR_BLUE_V1,
                    },
                },
            ],
        })
    }

    /// The current 30 Forest, 30 Island kernel-coverage calibration deck.
    /// This exists only to exercise the full pregame wiring and is not a
    /// competitive policy.
    pub fn kernel_basic_lands_wiring_only_v1() -> Result<Self, String> {
        Self::new_v1(MtgoNonModelPregameHeuristicProfileV1 {
            schema_version: MTGO_NON_MODEL_PREGAME_HEURISTIC_SCHEMA_V1,
            profile_id: KERNEL_BASICS_PROFILE_ID_V1.to_owned(),
            source_card_catalog_sha256: sha256_hex_v1(
                b"mtgo-fixed-30-forest-30-island-visible-label-catalog-v1",
            ),
            cards: vec![
                MtgoHeuristicCardFeatureV1 {
                    visible_card_name: "Forest".to_owned(),
                    kind: MtgoHeuristicCardKindV1::Land {
                        produced_color_mask: MTGO_HEURISTIC_COLOR_GREEN_V1,
                    },
                },
                MtgoHeuristicCardFeatureV1 {
                    visible_card_name: "Island".to_owned(),
                    kind: MtgoHeuristicCardKindV1::Land {
                        produced_color_mask: MTGO_HEURISTIC_COLOR_BLUE_V1,
                    },
                },
            ],
        })
    }

    pub fn profile_v1(&self) -> &MtgoNonModelPregameHeuristicProfileV1 {
        &self.profile
    }

    pub fn profile_commitment_sha256_v1(&self) -> &str {
        &self.profile_commitment_sha256
    }

    /// Compatibility envelope for the existing request schema. The returned
    /// record is explicitly not a native model checkpoint.
    pub fn compatibility_deployment_v1(&self) -> &MtgoExpectedModelDeploymentV1 {
        &self.compatibility_deployment
    }

    pub fn is_model_backed_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    fn require_request_deployment_v1(&self, observed: &str) -> Result<(), String> {
        let expected = model_deployment_commitment_v1(&self.compatibility_deployment)
            .map_err(|error| format!("commit heuristic compatibility deployment: {error}"))?;
        if observed != expected {
            return Err(
                "pregame request is not bound to this exact non-model heuristic profile".to_owned(),
            );
        }
        Ok(())
    }

    fn card_kind_v1(&self, name: &str) -> Result<&MtgoHeuristicCardKindV1, String> {
        self.cards_by_name
            .get(name)
            .ok_or_else(|| format!("heuristic profile has no exact feature row for {name:?}"))
    }
}

impl MtgoExternalCardAwarePregameScorerV4 for MtgoNonModelPregameHeuristicV1 {
    fn score_card_aware_pregame_v4(
        &mut self,
        request: &MtgoCardAwarePregameScoringRequestV4,
    ) -> Result<MtgoCardAwarePregameScoreResponseV4, String> {
        let request_commitment_sha256 = card_aware_pregame_scoring_request_commitment_v4(request)?;
        self.require_request_deployment_v1(&request.deployment_commitment_sha256)?;
        let features = request
            .ordered_visible_card_names
            .iter()
            .map(|name| self.card_kind_v1(name))
            .collect::<Result<Vec<_>, _>>()?;
        let profile_contains_only_lands = self
            .cards_by_name
            .values()
            .all(|feature| matches!(feature, MtgoHeuristicCardKindV1::Land { .. }));
        let keep_score = keep_score_v1(
            request.prospective_keep_size,
            &features,
            profile_contains_only_lands,
        );
        let logits_f32_bits = request
            .ordered_actions
            .iter()
            .map(|action| match action {
                MtgoPregameActionSemanticV1::Mulligan { .. } => 0.0_f32.to_bits(),
                MtgoPregameActionSemanticV1::KeepOpeningHand => keep_score.to_bits(),
            })
            .collect();
        let value = (keep_score / 64.0).clamp(-1.0, 1.0);
        Ok(MtgoCardAwarePregameScoreResponseV4 {
            schema_version: MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4,
            request_commitment_sha256,
            logits_f32_bits,
            value_f32_bits: value.to_bits(),
        })
    }
}

impl MtgoExternalCardAwareBottomingScorerV5 for MtgoNonModelPregameHeuristicV1 {
    fn score_card_aware_bottoming_v5(
        &mut self,
        request: &MtgoCardAwareBottomingScoringRequestV5,
    ) -> Result<MtgoCardAwareBottomingScoreResponseV5, String> {
        let request_commitment_sha256 =
            card_aware_bottoming_scoring_request_commitment_v5(request)?;
        self.require_request_deployment_v1(&request.deployment_commitment_sha256)?;
        for card in &request.ordered_visible_cards {
            self.card_kind_v1(&card.visible_card_name)?;
        }
        for card in &request.ordered_confirmed_bottomed_cards {
            self.card_kind_v1(&card.visible_card_name)?;
        }

        let available_colors = request
            .ordered_visible_cards
            .iter()
            .filter_map(
                |card| match self.cards_by_name.get(&card.visible_card_name) {
                    Some(MtgoHeuristicCardKindV1::Land {
                        produced_color_mask,
                    }) => Some(*produced_color_mask),
                    _ => None,
                },
            )
            .fold(0_u8, |mask, colors| mask | colors);
        let mut best_retained_value = f32::NEG_INFINITY;
        let logits_f32_bits = request
            .ordered_actions
            .iter()
            .map(|action| -> Result<u32, String> {
                let score = match action {
                    MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
                        adapter_object_id,
                        ..
                    } => {
                        let card = request
                            .ordered_visible_cards
                            .iter()
                            .find(|card| &card.adapter_object_id == adapter_object_id)
                            .ok_or("bottoming action does not resolve to one visible card")?;
                        let value = card_retention_value_v1(
                            self.card_kind_v1(&card.visible_card_name)?,
                            available_colors,
                        );
                        best_retained_value = best_retained_value.max(value);
                        -value
                    }
                    MtgoOfflineBottomingActionSemanticV1::SubmitBottoming => 1_000_000.0,
                    MtgoOfflineBottomingActionSemanticV1::CancelBottoming => -1_000_000.0,
                };
                Ok(score.to_bits())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let value = if best_retained_value.is_finite() {
            (best_retained_value / 64.0).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        Ok(MtgoCardAwareBottomingScoreResponseV5 {
            schema_version: MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5,
            request_commitment_sha256,
            logits_f32_bits,
            value_f32_bits: value.to_bits(),
        })
    }
}

pub fn non_model_pregame_heuristic_profile_commitment_v1(
    profile: &MtgoNonModelPregameHeuristicProfileV1,
) -> Result<String, String> {
    validate_heuristic_profile_v1(profile)?;
    canonical_json_commitment_v3(HEURISTIC_PROFILE_DOMAIN_V1, profile)
}

fn validate_heuristic_profile_v1(
    profile: &MtgoNonModelPregameHeuristicProfileV1,
) -> Result<(), String> {
    if profile.schema_version != MTGO_NON_MODEL_PREGAME_HEURISTIC_SCHEMA_V1
        || !valid_safe_identifier_v1(&profile.profile_id)
        || !valid_lower_sha256_v1(&profile.source_card_catalog_sha256)
        || profile.cards.is_empty()
        || profile.cards.len() > 512
    {
        return Err("non-model pregame heuristic profile header is invalid".to_owned());
    }
    let mut names = HashSet::new();
    for card in &profile.cards {
        if card.visible_card_name.is_empty()
            || card.visible_card_name.len() > 256
            || card.visible_card_name.trim() != card.visible_card_name
            || card.visible_card_name.chars().any(char::is_control)
            || !names.insert(card.visible_card_name.as_str())
        {
            return Err(
                "non-model pregame heuristic card names are invalid or duplicated".to_owned(),
            );
        }
        match card.kind {
            MtgoHeuristicCardKindV1::Land {
                produced_color_mask,
            } if produced_color_mask != 0
                && produced_color_mask & !MTGO_HEURISTIC_ALL_COLORS_V1 == 0 => {}
            MtgoHeuristicCardKindV1::Spell {
                mana_value,
                required_color_mask,
            } if mana_value <= 20 && required_color_mask & !MTGO_HEURISTIC_ALL_COLORS_V1 == 0 => {}
            _ => return Err("non-model pregame heuristic card features are invalid".to_owned()),
        }
    }
    Ok(())
}

fn compatibility_deployment_v1(
    profile: &MtgoNonModelPregameHeuristicProfileV1,
    profile_commitment_sha256: &str,
) -> Result<MtgoExpectedModelDeploymentV1, String> {
    let digest = |role: &str| -> Result<String, String> {
        #[derive(Serialize)]
        struct RecordV1<'a> {
            role: &'a str,
            profile_commitment_sha256: &'a str,
        }
        canonical_json_commitment_v3(
            HEURISTIC_COMPATIBILITY_DIGEST_DOMAIN_V1,
            &RecordV1 {
                role,
                profile_commitment_sha256,
            },
        )
    };
    let deployment = MtgoExpectedModelDeploymentV1 {
        schema_version: MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
        deployment_id: format!("non-model-pregame-heuristic-v1:{}", profile.profile_id),
        checkpoint: MtgoNativeCheckpointIdentityV1 {
            run_sha256: digest("not-a-model-run")?,
            checkpoint_manifest_sha256: digest("not-a-checkpoint-manifest")?,
            checkpoint_payload_sha256: digest("not-a-checkpoint-payload")?,
            train_state_sha256: digest("not-a-train-state")?,
            model_parameter_sha256: digest("not-model-parameters")?,
            generation_index: 0,
        },
        scorer_contract_sha256: digest("non-model-heuristic-scorer-contract")?,
    };
    model_deployment_commitment_v1(&deployment)
        .map_err(|error| format!("validate heuristic compatibility deployment: {error}"))?;
    Ok(deployment)
}

fn keep_score_v1(
    keep_size: u8,
    features: &[&MtgoHeuristicCardKindV1],
    profile_contains_only_lands: bool,
) -> f32 {
    if keep_size == 1 || profile_contains_only_lands {
        return 100.0;
    }
    let bottom_count = 7_u8.saturating_sub(keep_size);
    let land_count = features
        .iter()
        .filter(|feature| matches!(feature, MtgoHeuristicCardKindV1::Land { .. }))
        .count() as u8;
    let spell_count = 7_u8.saturating_sub(land_count);
    let (desired_min_lands, desired_max_lands) = desired_land_range_v1(keep_size);
    let minimum_retained_lands = land_count.saturating_sub(bottom_count);
    let maximum_retained_lands = land_count.min(keep_size);
    let land_distance = if maximum_retained_lands < desired_min_lands {
        desired_min_lands - maximum_retained_lands
    } else {
        minimum_retained_lands.saturating_sub(desired_max_lands)
    };

    let available_colors = features
        .iter()
        .filter_map(|feature| match feature {
            MtgoHeuristicCardKindV1::Land {
                produced_color_mask,
            } => Some(*produced_color_mask),
            _ => None,
        })
        .fold(0_u8, |mask, colors| mask | colors);
    let mut cheap_castable = 0_u8;
    let mut color_uncovered = 0_u8;
    for feature in features {
        if let MtgoHeuristicCardKindV1::Spell {
            mana_value,
            required_color_mask,
        } = feature
        {
            let covered = *required_color_mask & !available_colors == 0;
            cheap_castable += u8::from(*mana_value <= 3 && covered);
            color_uncovered += u8::from(!covered);
        }
    }

    let mut score = 10.0 - f32::from(land_distance) * 32.0;
    if spell_count <= bottom_count {
        score -= 12.0;
    }
    score += f32::from(cheap_castable.min(3)) * 3.0;
    score -= f32::from(color_uncovered.min(3)) * 2.0;
    score
}

fn desired_land_range_v1(keep_size: u8) -> (u8, u8) {
    match keep_size {
        1 | 2 => (1, 1),
        3 => (1, 2),
        4 => (1, 3),
        5 | 6 => (2, 4),
        _ => (2, 5),
    }
}

fn card_retention_value_v1(feature: &MtgoHeuristicCardKindV1, available_colors: u8) -> f32 {
    match feature {
        MtgoHeuristicCardKindV1::Land {
            produced_color_mask,
        } => 30.0 + produced_color_mask.count_ones() as f32 * 3.0,
        MtgoHeuristicCardKindV1::Spell {
            mana_value,
            required_color_mask,
        } => {
            let covered = *required_color_mask & !available_colors == 0;
            20.0 - f32::from(*mana_value) * 3.0
                + if *mana_value <= 2 { 12.0 } else { 0.0 }
                + if covered { 6.0 } else { -10.0 }
        }
    }
}

fn valid_safe_identifier_v1(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MtgoBottomingCardIdentitySourceV5, MtgoBottomingConfirmedCardV5, MtgoBottomingVisibleCardV5,
    };

    fn digest(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn profile_v1() -> MtgoNonModelPregameHeuristicProfileV1 {
        MtgoNonModelPregameHeuristicProfileV1 {
            schema_version: MTGO_NON_MODEL_PREGAME_HEURISTIC_SCHEMA_V1,
            profile_id: "heuristic-unit-test-v1".to_owned(),
            source_card_catalog_sha256: digest('a'),
            cards: vec![
                MtgoHeuristicCardFeatureV1 {
                    visible_card_name: "Plains".to_owned(),
                    kind: MtgoHeuristicCardKindV1::Land {
                        produced_color_mask: MTGO_HEURISTIC_COLOR_WHITE_V1,
                    },
                },
                MtgoHeuristicCardFeatureV1 {
                    visible_card_name: "Island".to_owned(),
                    kind: MtgoHeuristicCardKindV1::Land {
                        produced_color_mask: MTGO_HEURISTIC_COLOR_BLUE_V1,
                    },
                },
                MtgoHeuristicCardFeatureV1 {
                    visible_card_name: "Cheap".to_owned(),
                    kind: MtgoHeuristicCardKindV1::Spell {
                        mana_value: 1,
                        required_color_mask: MTGO_HEURISTIC_COLOR_WHITE_V1,
                    },
                },
                MtgoHeuristicCardFeatureV1 {
                    visible_card_name: "Huge".to_owned(),
                    kind: MtgoHeuristicCardKindV1::Spell {
                        mana_value: 5,
                        required_color_mask: MTGO_HEURISTIC_COLOR_WHITE_V1,
                    },
                },
            ],
        }
    }

    fn scorer_v1() -> MtgoNonModelPregameHeuristicV1 {
        MtgoNonModelPregameHeuristicV1::new_v1(profile_v1()).unwrap()
    }

    fn pregame_request_v4(
        scorer: &MtgoNonModelPregameHeuristicV1,
        prospective_keep_size: u8,
        names: &[&str],
    ) -> MtgoCardAwarePregameScoringRequestV4 {
        MtgoCardAwarePregameScoringRequestV4 {
            schema_version: MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4,
            source_capture_commitment_sha256: digest('1'),
            mulligan_measurement_commitment_sha256: digest('2'),
            visible_identity_measurement_commitment_sha256: digest('3'),
            mulligan_profile_set_commitment_sha256: digest('4'),
            visible_card_profile_commitment_sha256: digest('5'),
            prospective_keep_size,
            ordered_visible_card_names: names.iter().map(|name| (*name).to_owned()).collect(),
            ordered_actions: vec![
                MtgoPregameActionSemanticV1::Mulligan {
                    next_hand_size: prospective_keep_size - 1,
                },
                MtgoPregameActionSemanticV1::KeepOpeningHand,
            ],
            deployment_commitment_sha256: model_deployment_commitment_v1(
                scorer.compatibility_deployment_v1(),
            )
            .unwrap(),
        }
    }

    fn bottoming_request_v5(
        scorer: &MtgoNonModelPregameHeuristicV1,
        selected_count: u8,
        visible_names: &[&str],
        confirmed_names: &[&str],
    ) -> MtgoCardAwareBottomingScoringRequestV5 {
        let ordered_visible_cards = visible_names
            .iter()
            .enumerate()
            .map(|(current_ordinal, name)| MtgoBottomingVisibleCardV5 {
                adapter_object_id: format!("object-{current_ordinal}"),
                original_hand_ordinal: u8::try_from(current_ordinal).unwrap(),
                current_visible_ordinal: u8::try_from(current_ordinal).unwrap(),
                visible_card_name: (*name).to_owned(),
                identity_source: if selected_count < 6 {
                    MtgoBottomingCardIdentitySourceV5::CurrentVisibleTemplate
                } else {
                    MtgoBottomingCardIdentitySourceV5::ConfirmedActionHistory
                },
            })
            .collect::<Vec<_>>();
        let ordered_confirmed_bottomed_cards = confirmed_names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let original_ordinal = visible_names.len() + index;
                MtgoBottomingConfirmedCardV5 {
                    adapter_object_id: format!("object-{original_ordinal}"),
                    original_hand_ordinal: u8::try_from(original_ordinal).unwrap(),
                    selection_ordinal: u8::try_from(index + 1).unwrap(),
                    visible_card_name: (*name).to_owned(),
                }
            })
            .collect::<Vec<_>>();
        let mut ordered_actions = if selected_count == 6 {
            vec![
                MtgoOfflineBottomingActionSemanticV1::SubmitBottoming,
                MtgoOfflineBottomingActionSemanticV1::CancelBottoming,
            ]
        } else {
            ordered_visible_cards
                .iter()
                .map(
                    |card| MtgoOfflineBottomingActionSemanticV1::SelectForBottom {
                        adapter_object_id: card.adapter_object_id.clone(),
                        selection_ordinal: selected_count + 1,
                    },
                )
                .collect::<Vec<_>>()
        };
        if selected_count > 0 && selected_count < 6 {
            ordered_actions.push(MtgoOfflineBottomingActionSemanticV1::CancelBottoming);
        }
        MtgoCardAwareBottomingScoringRequestV5 {
            schema_version: MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5,
            source_capture_commitment_sha256: digest('1'),
            state_measurement_commitment_sha256: digest('2'),
            current_identity_measurement_commitment_sha256: (selected_count < 6)
                .then(|| digest('3')),
            state_profile_commitment_sha256: digest('4'),
            visible_card_profile_commitment_sha256: digest('5'),
            history_commitment_sha256: digest('6'),
            required_bottom_count: 6,
            selected_count,
            ordered_visible_cards,
            ordered_confirmed_bottomed_cards,
            ordered_actions,
            deployment_commitment_sha256: model_deployment_commitment_v1(
                scorer.compatibility_deployment_v1(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn profile_validation_rejects_ambiguous_or_invalid_features() {
        let mut duplicate = profile_v1();
        duplicate.cards.push(duplicate.cards[0].clone());
        assert!(MtgoNonModelPregameHeuristicV1::new_v1(duplicate).is_err());

        let mut zero_land_mask = profile_v1();
        zero_land_mask.cards[0].kind = MtgoHeuristicCardKindV1::Land {
            produced_color_mask: 0,
        };
        assert!(MtgoNonModelPregameHeuristicV1::new_v1(zero_land_mask).is_err());

        let mut unknown_color = profile_v1();
        unknown_color.cards[0].kind = MtgoHeuristicCardKindV1::Land {
            produced_color_mask: 1 << 7,
        };
        assert!(MtgoNonModelPregameHeuristicV1::new_v1(unknown_color).is_err());

        let mut excessive_mana_value = profile_v1();
        excessive_mana_value.cards[2].kind = MtgoHeuristicCardKindV1::Spell {
            mana_value: 21,
            required_color_mask: MTGO_HEURISTIC_COLOR_WHITE_V1,
        };
        assert!(MtgoNonModelPregameHeuristicV1::new_v1(excessive_mana_value).is_err());

        let mut malformed_source = profile_v1();
        malformed_source.source_card_catalog_sha256 = "A".repeat(64);
        assert!(MtgoNonModelPregameHeuristicV1::new_v1(malformed_source).is_err());
    }

    #[test]
    fn compatibility_deployment_is_profile_bound_and_explicitly_non_model() {
        let scorer = scorer_v1();
        assert_eq!(
            scorer
                .compatibility_deployment_v1()
                .checkpoint
                .generation_index,
            0
        );
        assert!(scorer
            .compatibility_deployment_v1()
            .deployment_id
            .starts_with("non-model-pregame-heuristic-v1:"));
        assert!(!scorer.is_model_backed_v1());
        assert!(!scorer.safe_for_live_input_v1());

        let mut changed = profile_v1();
        changed.cards[2].kind = MtgoHeuristicCardKindV1::Spell {
            mana_value: 2,
            required_color_mask: MTGO_HEURISTIC_COLOR_WHITE_V1,
        };
        let changed = MtgoNonModelPregameHeuristicV1::new_v1(changed).unwrap();
        assert_ne!(
            scorer.profile_commitment_sha256_v1(),
            changed.profile_commitment_sha256_v1()
        );
        assert_ne!(
            model_deployment_commitment_v1(scorer.compatibility_deployment_v1()).unwrap(),
            model_deployment_commitment_v1(changed.compatibility_deployment_v1()).unwrap()
        );
    }

    #[test]
    fn keeps_balanced_hand_and_mulligans_zero_or_seven_land_hands() {
        let mut scorer = scorer_v1();
        let balanced = pregame_request_v4(
            &scorer,
            7,
            &[
                "Plains", "Island", "Plains", "Cheap", "Cheap", "Huge", "Cheap",
            ],
        );
        let balanced_response = scorer.score_card_aware_pregame_v4(&balanced).unwrap();
        assert!(
            f32::from_bits(balanced_response.logits_f32_bits[1])
                > f32::from_bits(balanced_response.logits_f32_bits[0])
        );

        let no_lands = pregame_request_v4(
            &scorer,
            7,
            &["Cheap", "Cheap", "Cheap", "Huge", "Huge", "Cheap", "Huge"],
        );
        let no_lands_response = scorer.score_card_aware_pregame_v4(&no_lands).unwrap();
        assert!(
            f32::from_bits(no_lands_response.logits_f32_bits[0])
                > f32::from_bits(no_lands_response.logits_f32_bits[1])
        );

        let all_lands = pregame_request_v4(
            &scorer,
            7,
            &[
                "Plains", "Island", "Plains", "Island", "Plains", "Island", "Plains",
            ],
        );
        let all_lands_response = scorer.score_card_aware_pregame_v4(&all_lands).unwrap();
        assert!(
            f32::from_bits(all_lands_response.logits_f32_bits[0])
                > f32::from_bits(all_lands_response.logits_f32_bits[1])
        );
    }

    #[test]
    fn one_card_hand_is_always_kept() {
        let mut scorer = scorer_v1();
        let request = pregame_request_v4(
            &scorer,
            1,
            &["Huge", "Huge", "Huge", "Huge", "Huge", "Huge", "Huge"],
        );
        let response = scorer.score_card_aware_pregame_v4(&request).unwrap();
        assert!(
            f32::from_bits(response.logits_f32_bits[1])
                > f32::from_bits(response.logits_f32_bits[0])
        );
    }

    #[test]
    fn kernel_basic_lands_profile_keeps_the_seven_card_calibration_hand() {
        let mut scorer =
            MtgoNonModelPregameHeuristicV1::kernel_basic_lands_wiring_only_v1().unwrap();
        let request = pregame_request_v4(
            &scorer,
            7,
            &[
                "Forest", "Island", "Forest", "Forest", "Forest", "Forest", "Island",
            ],
        );
        let response = scorer.score_card_aware_pregame_v4(&request).unwrap();
        assert!(
            f32::from_bits(response.logits_f32_bits[1])
                > f32::from_bits(response.logits_f32_bits[0])
        );
        assert!(!scorer.is_model_backed_v1());
        assert!(!scorer.safe_for_live_input_v1());
    }

    #[test]
    fn bottoming_selects_lowest_retention_card_and_never_cancel() {
        let mut scorer = scorer_v1();
        let request = bottoming_request_v5(
            &scorer,
            5,
            &["Huge", "Plains"],
            &["Island", "Plains", "Island", "Plains", "Island"],
        );
        let response = scorer.score_card_aware_bottoming_v5(&request).unwrap();
        let logits = response
            .logits_f32_bits
            .iter()
            .map(|bits| f32::from_bits(*bits))
            .collect::<Vec<_>>();
        assert_eq!(logits.len(), 3);
        assert!(logits[0] > logits[1]);
        assert!(logits[0] > logits[2]);
    }

    #[test]
    fn completed_bottoming_selects_submit() {
        let mut scorer = scorer_v1();
        let request = bottoming_request_v5(
            &scorer,
            6,
            &["Plains"],
            &["Island", "Plains", "Island", "Plains", "Island", "Plains"],
        );
        let response = scorer.score_card_aware_bottoming_v5(&request).unwrap();
        assert!(
            f32::from_bits(response.logits_f32_bits[0])
                > f32::from_bits(response.logits_f32_bits[1])
        );
    }

    #[test]
    fn exact_labels_and_deployment_commitment_are_required() {
        let mut scorer = scorer_v1();
        let unknown = pregame_request_v4(
            &scorer,
            7,
            &[
                "Unknown", "Plains", "Island", "Cheap", "Cheap", "Huge", "Cheap",
            ],
        );
        assert!(scorer.score_card_aware_pregame_v4(&unknown).is_err());

        let mut wrong_deployment = pregame_request_v4(
            &scorer,
            7,
            &[
                "Plains", "Island", "Plains", "Cheap", "Cheap", "Huge", "Cheap",
            ],
        );
        wrong_deployment.deployment_commitment_sha256 = digest('f');
        assert!(scorer
            .score_card_aware_pregame_v4(&wrong_deployment)
            .is_err());
    }

    #[test]
    fn repeated_scoring_is_bit_deterministic_and_request_bound() {
        let scorer = scorer_v1();
        let request = bottoming_request_v5(
            &scorer,
            0,
            &[
                "Huge", "Cheap", "Plains", "Island", "Cheap", "Plains", "Island",
            ],
            &[],
        );
        let mut left = scorer.clone();
        let mut right = scorer;
        let left_response = left.score_card_aware_bottoming_v5(&request).unwrap();
        let right_response = right.score_card_aware_bottoming_v5(&request).unwrap();
        assert_eq!(left_response, right_response);
        assert_eq!(
            left_response.request_commitment_sha256,
            card_aware_bottoming_scoring_request_commitment_v5(&request).unwrap()
        );
    }
}
