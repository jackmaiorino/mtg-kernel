use crate::{
    actuator::competitive_pregame_expected_postcondition_v1,
    validate_competitive_native_pregame_model_input_v1,
    validate_competitive_native_sideboard_model_input_v1,
    validate_competitive_native_sideboard_model_selection_v1,
    CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1,
    CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1,
    MtgoCompetitiveNativePregameActionV1, MtgoCompetitiveNativePregameModelInputV1,
    MtgoCompetitiveNativeSideboardCardCountV1, MtgoCompetitiveNativeSideboardConfigurationV1,
    MtgoCompetitiveNativeSideboardModelInputV1, MtgoCompetitivePregameExpectedPostconditionV1,
    MtgoCompetitivePregameSelectedActionV1, MtgoCompetitivePregameStageV1,
};
use mtgo_blackbox_v1::{
    MtgoCompetitiveDeckCardCountV1, MtgoCompetitiveDeckConfigurationV1,
    MtgoCompetitivePregameStageLabelV1, MtgoCompetitiveSideboardSelectionV1,
    ValidatedMtgoCompetitiveDeckManifestV1, MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const NATIVE_PREGAME_SEMANTIC_RESOLUTION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-pregame-semantic-resolution-v1";
const NATIVE_SIDEBOARD_SEMANTIC_RESOLUTION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-sideboard-semantic-resolution-v1";

/// Coordinate-free interpretation of one checked-untrusted pregame score.
///
/// This proves only that the selected native action maps to one exact action
/// in the player-visible request. It has no retained session, visible control
/// rectangle, checkpoint-origin proof, input method, event-entry authority, or
/// spending authority.
pub struct CheckedUntrustedMtgoCompetitivePregameSemanticResolutionV1 {
    selected_index: usize,
    native_action: MtgoCompetitiveNativePregameActionV1,
    selected_action: MtgoCompetitivePregameSelectedActionV1,
    expected_postcondition: MtgoCompetitivePregameExpectedPostconditionV1,
    model_input_commitment_sha256: String,
    checked_selection_commitment_sha256: String,
    semantic_resolution_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitivePregameSemanticResolutionV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn native_action_v1(&self) -> &MtgoCompetitiveNativePregameActionV1 {
        &self.native_action
    }

    pub fn selected_action_v1(&self) -> &MtgoCompetitivePregameSelectedActionV1 {
        &self.selected_action
    }

    pub fn expected_postcondition_v1(&self) -> &MtgoCompetitivePregameExpectedPostconditionV1 {
        &self.expected_postcondition
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.model_input_commitment_sha256
    }

    pub fn checked_selection_commitment_sha256_v1(&self) -> &str {
        &self.checked_selection_commitment_sha256
    }

    pub fn semantic_resolution_commitment_sha256_v1(&self) -> &str {
        &self.semantic_resolution_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Resolves one structurally checked pregame selection into the adapter's
/// coordinate-free event semantic and its exact visible postcondition.
///
/// The selected card name, when present, is copied from the player-visible
/// request. No adapter card ID, pixel, rectangle, process identity, or hidden
/// game state enters the result.
pub fn resolve_checked_untrusted_competitive_native_pregame_selection_v1(
    model_input: &MtgoCompetitiveNativePregameModelInputV1,
    selection: &CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1,
) -> Result<CheckedUntrustedMtgoCompetitivePregameSemanticResolutionV1, String> {
    validate_competitive_native_pregame_model_input_v1(model_input)?;
    let model_input_commitment_sha256 =
        crate::competitive_native_pregame_model_input_commitment_v1(model_input)?;
    if selection.model_input_commitment_sha256_v1() != model_input_commitment_sha256 {
        return Err("native pregame semantic resolution changed the exact model input".to_owned());
    }
    let selected_index = selection.selected_index_v1();
    let native_action = model_input
        .ordered_actions
        .get(selected_index)
        .filter(|action| *action == selection.selected_action_v1())
        .cloned()
        .ok_or("native pregame semantic resolution changed the selected action index")?;
    let selected_action = match native_action {
        MtgoCompetitiveNativePregameActionV1::KeepOpeningHand => {
            MtgoCompetitivePregameSelectedActionV1::KeepOpeningHand
        }
        MtgoCompetitiveNativePregameActionV1::Mulligan { next_hand_size } => {
            MtgoCompetitivePregameSelectedActionV1::Mulligan { next_hand_size }
        }
        MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot } => {
            let card = model_input
                .ordered_visible_cards
                .get(usize::from(card_slot))
                .filter(|card| card.card_slot == card_slot && !card.selected_for_bottom)
                .ok_or("native pregame bottom selection does not resolve to one visible card")?;
            MtgoCompetitivePregameSelectedActionV1::SelectForBottom {
                card_slot,
                visible_card_name: card.visible_card_name.clone(),
            }
        }
        MtgoCompetitiveNativePregameActionV1::SubmitBottoming => {
            MtgoCompetitivePregameSelectedActionV1::SubmitBottoming
        }
    };
    let expected_postcondition = competitive_pregame_expected_postcondition_v1(
        native_pregame_stage_label_v1(model_input.stage),
        &selected_action,
    )?;
    let selected_action_json = serde_json::to_vec(&selected_action)
        .map_err(|error| format!("serialize resolved native pregame action: {error}"))?;
    let expected_postcondition_json = serde_json::to_vec(&expected_postcondition)
        .map_err(|error| format!("serialize resolved native pregame postcondition: {error}"))?;
    let checked_selection_commitment_sha256 = selection.selection_commitment_sha256_v1().to_owned();
    let semantic_resolution_commitment_sha256 = commitment_v1(
        NATIVE_PREGAME_SEMANTIC_RESOLUTION_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            selection.deployment_commitment_sha256_v1().as_bytes(),
            checked_selection_commitment_sha256.as_bytes(),
            &(selected_index as u64).to_be_bytes(),
            &selected_action_json,
            &expected_postcondition_json,
            b"player_visible_coordinate_free_semantic_resolution_no_session_no_input",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitivePregameSemanticResolutionV1 {
        selected_index,
        native_action,
        selected_action,
        expected_postcondition,
        model_input_commitment_sha256,
        checked_selection_commitment_sha256,
        semantic_resolution_commitment_sha256,
    })
}

fn native_pregame_stage_label_v1(
    stage: MtgoCompetitivePregameStageV1,
) -> MtgoCompetitivePregameStageLabelV1 {
    match stage {
        MtgoCompetitivePregameStageV1::MulliganChoice {
            prospective_keep_size,
        } => MtgoCompetitivePregameStageLabelV1::MulliganChoice {
            prospective_keep_size,
        },
        MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        } => MtgoCompetitivePregameStageLabelV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        },
        MtgoCompetitivePregameStageV1::GameplayReady => {
            MtgoCompetitivePregameStageLabelV1::GameplayReady
        }
    }
}

/// Exact adapter-local mapping of one visible-name sideboard target back to
/// the submitted deck manifest. Card database IDs remain private because they
/// are routing metadata, not model-facing game information.
pub struct CheckedUntrustedMtgoCompetitiveSideboardSemanticResolutionV1 {
    visible_target_configuration: MtgoCompetitiveNativeSideboardConfigurationV1,
    model_input_commitment_sha256: String,
    checked_selection_commitment_sha256: String,
    adapter_target_configuration_commitment_sha256: String,
    semantic_resolution_commitment_sha256: String,
    no_changes_selected: bool,
}

impl CheckedUntrustedMtgoCompetitiveSideboardSemanticResolutionV1 {
    pub fn visible_target_configuration_v1(
        &self,
    ) -> &MtgoCompetitiveNativeSideboardConfigurationV1 {
        &self.visible_target_configuration
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.model_input_commitment_sha256
    }

    pub fn checked_selection_commitment_sha256_v1(&self) -> &str {
        &self.checked_selection_commitment_sha256
    }

    pub fn adapter_target_configuration_commitment_sha256_v1(&self) -> &str {
        &self.adapter_target_configuration_commitment_sha256
    }

    pub fn semantic_resolution_commitment_sha256_v1(&self) -> &str {
        &self.semantic_resolution_commitment_sha256
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

/// Resolves one checked-untrusted visible-name target against the exact local
/// submitted deck inventory. The model never receives the adapter-local card
/// IDs used by the lower sideboard planner.
pub fn resolve_checked_untrusted_competitive_native_sideboard_selection_v1(
    model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
    selection: &CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    source_snapshot_commitment_sha256: &str,
    policy_deployment_commitment_sha256: &str,
) -> Result<CheckedUntrustedMtgoCompetitiveSideboardSemanticResolutionV1, String> {
    validate_competitive_native_sideboard_model_input_v1(model_input)?;
    validate_competitive_native_sideboard_model_selection_v1(
        model_input,
        selection.selection_v1(),
    )?;
    let model_input_commitment_sha256 =
        crate::competitive_native_sideboard_model_input_commitment_v1(model_input)?;
    if selection.model_input_commitment_sha256_v1() != model_input_commitment_sha256
        || selection.deployment_commitment_sha256_v1() != policy_deployment_commitment_sha256
    {
        return Err(
            "native sideboard semantic resolution changed the exact model input or deployment"
                .to_owned(),
        );
    }
    require_sha256_v1(
        source_snapshot_commitment_sha256,
        "native sideboard source snapshot commitment",
    )?;
    require_sha256_v1(
        policy_deployment_commitment_sha256,
        "native sideboard policy deployment commitment",
    )?;

    let manifest_index = manifest_visible_name_index_v1(manifest.configuration_v1())?;
    require_visible_inventory_matches_manifest_v1(
        &model_input.current_configuration,
        &manifest_index,
    )?;
    let visible_target_configuration = selection.selection_v1().target_configuration.clone();
    let lower_target_configuration =
        lower_configuration_from_visible_v1(&visible_target_configuration, &manifest_index)?;
    let lower_selection = MtgoCompetitiveSideboardSelectionV1 {
        schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
        source_snapshot_commitment_sha256: source_snapshot_commitment_sha256.to_owned(),
        deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
        policy_deployment_commitment_sha256: policy_deployment_commitment_sha256.to_owned(),
        target_configuration: lower_target_configuration,
    };
    let lower_selection_json = serde_json::to_vec(&lower_selection)
        .map_err(|error| format!("serialize resolved native sideboard selection: {error}"))?;
    let adapter_target_configuration_commitment_sha256 = commitment_v1(
        NATIVE_SIDEBOARD_SEMANTIC_RESOLUTION_DOMAIN_V1,
        &[
            &lower_selection_json,
            b"private_adapter_target_configuration_commitment_no_ids_exposed",
        ],
    );
    let checked_selection_commitment_sha256 = selection
        .checked_selection_commitment_sha256_v1()
        .to_owned();
    let no_changes_selected = visible_target_configuration == model_input.current_configuration;
    let semantic_resolution_commitment_sha256 = commitment_v1(
        NATIVE_SIDEBOARD_SEMANTIC_RESOLUTION_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            selection.deployment_commitment_sha256_v1().as_bytes(),
            selection.model_selection_commitment_sha256_v1().as_bytes(),
            checked_selection_commitment_sha256.as_bytes(),
            source_snapshot_commitment_sha256.as_bytes(),
            manifest.manifest_commitment_sha256().as_bytes(),
            adapter_target_configuration_commitment_sha256.as_bytes(),
            &[u8::from(no_changes_selected)],
            b"visible_name_target_to_private_adapter_id_plan_no_session_no_input_no_submit",
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitiveSideboardSemanticResolutionV1 {
            visible_target_configuration,
            model_input_commitment_sha256,
            checked_selection_commitment_sha256,
            adapter_target_configuration_commitment_sha256,
            semantic_resolution_commitment_sha256,
            no_changes_selected,
        },
    )
}

#[derive(Clone, Copy)]
struct ManifestCardIdentityV1 {
    card_db_id: u16,
    combined_count: u16,
}

fn manifest_visible_name_index_v1(
    configuration: &MtgoCompetitiveDeckConfigurationV1,
) -> Result<BTreeMap<String, ManifestCardIdentityV1>, String> {
    let mut by_name = BTreeMap::<String, ManifestCardIdentityV1>::new();
    for card in configuration
        .mainboard
        .iter()
        .chain(&configuration.sideboard)
    {
        match by_name.get_mut(&card.card_name) {
            Some(identity) if identity.card_db_id == card.card_db_id => {
                identity.combined_count = identity
                    .combined_count
                    .checked_add(card.count)
                    .ok_or("submitted deck visible-name inventory count overflow")?;
            }
            Some(_) => {
                return Err(
                    "submitted deck maps one visible card name to multiple adapter card IDs"
                        .to_owned(),
                )
            }
            None => {
                by_name.insert(
                    card.card_name.clone(),
                    ManifestCardIdentityV1 {
                        card_db_id: card.card_db_id,
                        combined_count: card.count,
                    },
                );
            }
        }
    }
    Ok(by_name)
}

fn require_visible_inventory_matches_manifest_v1(
    configuration: &MtgoCompetitiveNativeSideboardConfigurationV1,
    manifest_index: &BTreeMap<String, ManifestCardIdentityV1>,
) -> Result<(), String> {
    let visible_inventory = combined_visible_inventory_v1(configuration)?;
    if visible_inventory.len() != manifest_index.len()
        || visible_inventory.iter().any(|(name, count)| {
            manifest_index
                .get(name)
                .is_none_or(|identity| identity.combined_count != *count)
        })
    {
        return Err(
            "native sideboard player-visible inventory differs from the submitted deck manifest"
                .to_owned(),
        );
    }
    Ok(())
}

fn combined_visible_inventory_v1(
    configuration: &MtgoCompetitiveNativeSideboardConfigurationV1,
) -> Result<BTreeMap<String, u16>, String> {
    let mut combined = BTreeMap::<String, u16>::new();
    for card in configuration
        .mainboard
        .iter()
        .chain(&configuration.sideboard)
    {
        let prior = combined.get(&card.visible_card_name).copied().unwrap_or(0);
        combined.insert(
            card.visible_card_name.clone(),
            prior
                .checked_add(card.count)
                .ok_or("native sideboard visible inventory count overflow")?,
        );
    }
    Ok(combined)
}

fn lower_configuration_from_visible_v1(
    configuration: &MtgoCompetitiveNativeSideboardConfigurationV1,
    manifest_index: &BTreeMap<String, ManifestCardIdentityV1>,
) -> Result<MtgoCompetitiveDeckConfigurationV1, String> {
    let convert = |cards: &[MtgoCompetitiveNativeSideboardCardCountV1]| {
        let mut lower = cards
            .iter()
            .map(|card| {
                let identity = manifest_index.get(&card.visible_card_name).ok_or_else(|| {
                    "native sideboard target contains a card absent from the submitted deck"
                        .to_owned()
                })?;
                Ok(MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: identity.card_db_id,
                    card_name: card.visible_card_name.clone(),
                    count: card.count,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        lower.sort_by_key(|card| card.card_db_id);
        Ok::<_, String>(lower)
    };
    Ok(MtgoCompetitiveDeckConfigurationV1 {
        mainboard: convert(&configuration.mainboard)?,
        sideboard: convert(&configuration.sideboard)?,
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
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        score_checked_untrusted_competitive_native_pregame_v1,
        score_checked_untrusted_competitive_native_sideboard_v1,
        MtgoCompetitiveNativePregameCardV1, MtgoCompetitiveNativePregameScoreResponseV1,
        MtgoCompetitiveNativePregameScorerV1, MtgoCompetitiveNativeSideboardModelSelectionV1,
        MtgoCompetitiveNativeSideboardScoreResponseV1, MtgoCompetitiveNativeSideboardScorerV1,
        MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
    };
    use mtgo_blackbox_v1::{
        validate_competitive_deck_manifest_v1, MtgoCompetitiveDeckManifestV1,
        MtgoCompetitivePregamePlayDrawV1,
    };

    fn card_v1(name: &str, count: u16) -> MtgoCompetitiveDeckCardCountV1 {
        MtgoCompetitiveDeckCardCountV1 {
            card_db_id: mtg_kernel::card_def::card_id_by_name(name).unwrap(),
            card_name: name.to_owned(),
            count,
        }
    }

    fn manifest_v1() -> ValidatedMtgoCompetitiveDeckManifestV1 {
        validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            deck_list_sha256: "1".repeat(64),
            format_sha256: "2".repeat(64),
            starting_mainboard_count: 60,
            starting_sideboard_count: 15,
            configuration: MtgoCompetitiveDeckConfigurationV1 {
                mainboard: {
                    let mut cards = vec![card_v1("Lightning Bolt", 4), card_v1("Mountain", 56)];
                    cards.sort_by_key(|card| card.card_db_id);
                    cards
                },
                sideboard: vec![card_v1("Lightning Bolt", 15)],
            },
        })
        .unwrap()
    }

    fn visible_deck_v1() -> MtgoCompetitiveNativeSideboardConfigurationV1 {
        crate::visible_native_sideboard_configuration_v1(manifest_v1().configuration_v1()).unwrap()
    }

    fn pregame_input_v1(
        stage: MtgoCompetitivePregameStageV1,
    ) -> MtgoCompetitiveNativePregameModelInputV1 {
        let (prospective_keep_size, required_bottom_count, selected_bottom_count, actions) =
            match stage {
                MtgoCompetitivePregameStageV1::MulliganChoice {
                    prospective_keep_size,
                } => (
                    Some(prospective_keep_size),
                    0,
                    0,
                    vec![
                        MtgoCompetitiveNativePregameActionV1::KeepOpeningHand,
                        MtgoCompetitiveNativePregameActionV1::Mulligan {
                            next_hand_size: prospective_keep_size - 1,
                        },
                    ],
                ),
                MtgoCompetitivePregameStageV1::LondonBottoming {
                    required_bottom_count,
                    selected_bottom_count,
                } => {
                    let mut actions =
                        (0_u8..7)
                            .filter(|slot| usize::from(*slot) >= usize::from(selected_bottom_count))
                            .map(|card_slot| {
                                MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot }
                            })
                            .collect::<Vec<_>>();
                    if selected_bottom_count == required_bottom_count {
                        actions.push(MtgoCompetitiveNativePregameActionV1::SubmitBottoming);
                    }
                    (None, required_bottom_count, selected_bottom_count, actions)
                }
                MtgoCompetitivePregameStageV1::GameplayReady => unreachable!(),
            };
        MtgoCompetitiveNativePregameModelInputV1 {
            game_number: 1,
            play_draw: MtgoCompetitivePregamePlayDrawV1::OnPlay,
            acting_player_games_won: 0,
            opponent_games_won: 0,
            player_known_deck_configuration: visible_deck_v1(),
            stage,
            prospective_keep_size,
            required_bottom_count,
            selected_bottom_count,
            ordered_visible_cards: (0_u8..7)
                .map(|card_slot| MtgoCompetitiveNativePregameCardV1 {
                    card_slot,
                    visible_card_name: format!("Visible Card {card_slot}"),
                    selected_for_bottom: card_slot < selected_bottom_count,
                })
                .collect(),
            ordered_confirmed_bottom_slots: (0_u8..selected_bottom_count).collect(),
            ordered_actions: actions,
        }
    }

    struct PregameScorerV1 {
        selected_index: usize,
    }

    impl MtgoCompetitiveNativePregameScorerV1 for PregameScorerV1 {
        fn score_pregame_v1(
            &mut self,
            input: &MtgoCompetitiveNativePregameModelInputV1,
            input_commitment: &str,
            deployment_commitment: &str,
        ) -> Result<MtgoCompetitiveNativePregameScoreResponseV1, String> {
            let mut logits = vec![0.0_f32; input.ordered_actions.len()];
            logits[self.selected_index] = 1.0;
            Ok(MtgoCompetitiveNativePregameScoreResponseV1 {
                schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
                model_input_commitment_sha256: input_commitment.to_owned(),
                deployment_commitment_sha256: deployment_commitment.to_owned(),
                ordered_action_logits_f32_bits: logits.into_iter().map(f32::to_bits).collect(),
                value_f32_bits: 0.0_f32.to_bits(),
            })
        }
    }

    struct SideboardScorerV1 {
        target: MtgoCompetitiveNativeSideboardConfigurationV1,
    }

    impl MtgoCompetitiveNativeSideboardScorerV1 for SideboardScorerV1 {
        fn score_sideboard_v1(
            &mut self,
            _input: &MtgoCompetitiveNativeSideboardModelInputV1,
            input_commitment: &str,
            deployment_commitment: &str,
        ) -> Result<MtgoCompetitiveNativeSideboardScoreResponseV1, String> {
            Ok(MtgoCompetitiveNativeSideboardScoreResponseV1 {
                schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
                model_input_commitment_sha256: input_commitment.to_owned(),
                deployment_commitment_sha256: deployment_commitment.to_owned(),
                selection: MtgoCompetitiveNativeSideboardModelSelectionV1 {
                    target_configuration: self.target.clone(),
                },
                value_f32_bits: 0.0_f32.to_bits(),
            })
        }
    }

    #[test]
    fn pregame_resolution_maps_mulligan_and_visible_bottom_card_exactly() {
        let mulligan_input = pregame_input_v1(MtgoCompetitivePregameStageV1::MulliganChoice {
            prospective_keep_size: 7,
        });
        let mut scorer = PregameScorerV1 { selected_index: 1 };
        let selection = score_checked_untrusted_competitive_native_pregame_v1(
            &mulligan_input,
            &"3".repeat(64),
            &mut scorer,
        )
        .unwrap();
        let resolved = resolve_checked_untrusted_competitive_native_pregame_selection_v1(
            &mulligan_input,
            &selection,
        )
        .unwrap();
        assert_eq!(
            resolved.selected_action_v1(),
            &MtgoCompetitivePregameSelectedActionV1::Mulligan { next_hand_size: 6 }
        );
        assert_eq!(
            resolved.expected_postcondition_v1(),
            &MtgoCompetitivePregameExpectedPostconditionV1::MulliganChoice {
                prospective_keep_size: 6
            }
        );

        let bottom_input = pregame_input_v1(MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count: 2,
            selected_bottom_count: 1,
        });
        let mut scorer = PregameScorerV1 { selected_index: 2 };
        let selection = score_checked_untrusted_competitive_native_pregame_v1(
            &bottom_input,
            &"3".repeat(64),
            &mut scorer,
        )
        .unwrap();
        let resolved = resolve_checked_untrusted_competitive_native_pregame_selection_v1(
            &bottom_input,
            &selection,
        )
        .unwrap();
        assert_eq!(
            resolved.selected_action_v1(),
            &MtgoCompetitivePregameSelectedActionV1::SelectForBottom {
                card_slot: 3,
                visible_card_name: "Visible Card 3".to_owned(),
            }
        );
        assert!(!resolved.safe_for_live_input_v1());
        assert!(!resolved.permits_event_session_recovery_v1());
    }

    #[test]
    fn sideboard_resolution_maps_visible_names_to_private_manifest_ids() {
        let manifest = manifest_v1();
        let input = MtgoCompetitiveNativeSideboardModelInputV1 {
            next_game_number: 2,
            acting_player_games_won: 1,
            opponent_games_won: 0,
            current_configuration: visible_deck_v1(),
        };
        let mut target = input.current_configuration.clone();
        target.mainboard[0].count += 1;
        target.sideboard[0].count -= 1;
        let mut scorer = SideboardScorerV1 { target };
        let selection = score_checked_untrusted_competitive_native_sideboard_v1(
            &input,
            &"3".repeat(64),
            &mut scorer,
        )
        .unwrap();
        let resolved = resolve_checked_untrusted_competitive_native_sideboard_selection_v1(
            &input,
            &selection,
            &manifest,
            &"4".repeat(64),
            &"3".repeat(64),
        )
        .unwrap();
        assert!(!resolved.no_changes_selected_v1());
        assert_eq!(
            resolved.visible_target_configuration_v1(),
            &selection.selection_v1().target_configuration
        );
        assert_eq!(
            resolved
                .adapter_target_configuration_commitment_sha256_v1()
                .len(),
            64
        );
    }

    #[test]
    fn sideboard_resolution_rejects_crossed_manifest_or_deployment() {
        let manifest = manifest_v1();
        let input = MtgoCompetitiveNativeSideboardModelInputV1 {
            next_game_number: 2,
            acting_player_games_won: 1,
            opponent_games_won: 0,
            current_configuration: visible_deck_v1(),
        };
        let mut scorer = SideboardScorerV1 {
            target: input.current_configuration.clone(),
        };
        let selection = score_checked_untrusted_competitive_native_sideboard_v1(
            &input,
            &"3".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert!(
            resolve_checked_untrusted_competitive_native_sideboard_selection_v1(
                &input,
                &selection,
                &manifest,
                &"4".repeat(64),
                &"5".repeat(64),
            )
            .is_err()
        );

        let mut crossed_input = input;
        crossed_input.current_configuration.mainboard[0].count -= 1;
        crossed_input.current_configuration.mainboard[1].count += 1;
        let mut scorer = SideboardScorerV1 {
            target: crossed_input.current_configuration.clone(),
        };
        let crossed_selection = score_checked_untrusted_competitive_native_sideboard_v1(
            &crossed_input,
            &"3".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert!(
            resolve_checked_untrusted_competitive_native_sideboard_selection_v1(
                &crossed_input,
                &crossed_selection,
                &manifest,
                &"4".repeat(64),
                &"3".repeat(64),
            )
            .is_err()
        );
    }
}
