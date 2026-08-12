use mtgo_blackbox_v1::MtgoCompetitiveDeckConfigurationV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

const NATIVE_SIDEBOARD_MODEL_INPUT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-sideboard-model-input-v1";
const NATIVE_SIDEBOARD_MODEL_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-sideboard-model-selection-v1";
const MAX_CARD_KINDS_V1: usize = 256;
const MAX_TOTAL_CARDS_V1: u32 = 350;
const MIN_MAINBOARD_CARDS_V1: u32 = 60;
const MAX_SIDEBOARD_CARDS_V1: u32 = 15;

/// One player-visible card identity and count. Kernel card-database IDs are
/// deliberately absent because they are adapter metadata, not game
/// information shown to the seated player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativeSideboardCardCountV1 {
    pub visible_card_name: String,
    pub count: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativeSideboardConfigurationV1 {
    pub mainboard: Vec<MtgoCompetitiveNativeSideboardCardCountV1>,
    pub sideboard: Vec<MtgoCompetitiveNativeSideboardCardCountV1>,
}

/// Coordinate-free sideboard game information for a future native scorer.
/// The score must eventually come from a separately corroborated visible
/// source. This type itself carries no provenance or live authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativeSideboardModelInputV1 {
    pub next_game_number: u8,
    pub acting_player_games_won: u8,
    pub opponent_games_won: u8,
    pub current_configuration: MtgoCompetitiveNativeSideboardConfigurationV1,
}

/// One model-selected visible target multiset. An unchanged target is the
/// explicit no-changes action. This is ordinary data and cannot move a card or
/// submit the MTGO sideboard screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativeSideboardModelSelectionV1 {
    pub target_configuration: MtgoCompetitiveNativeSideboardConfigurationV1,
}

pub fn visible_native_sideboard_configuration_v1(
    configuration: &MtgoCompetitiveDeckConfigurationV1,
) -> Result<MtgoCompetitiveNativeSideboardConfigurationV1, String> {
    let convert = |cards: &[mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1]| {
        let mut visible = cards
            .iter()
            .map(|card| MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: card.card_name.clone(),
                count: card.count,
            })
            .collect::<Vec<_>>();
        visible.sort_by(|left, right| left.visible_card_name.cmp(&right.visible_card_name));
        visible
    };
    let visible = MtgoCompetitiveNativeSideboardConfigurationV1 {
        mainboard: convert(&configuration.mainboard),
        sideboard: convert(&configuration.sideboard),
    };
    validate_configuration_v1(&visible)?;
    Ok(visible)
}

pub fn validate_competitive_native_sideboard_model_input_v1(
    input: &MtgoCompetitiveNativeSideboardModelInputV1,
) -> Result<(), String> {
    match input.next_game_number {
        2 if input
            .acting_player_games_won
            .checked_add(input.opponent_games_won)
            == Some(1)
            && input.acting_player_games_won <= 1
            && input.opponent_games_won <= 1 => {}
        3 if input.acting_player_games_won == 1 && input.opponent_games_won == 1 => {}
        _ => {
            return Err(
                "native sideboard game number and player-visible match score are inconsistent"
                    .to_owned(),
            )
        }
    }
    if partition_total_v1(&input.current_configuration.mainboard)? < MIN_MAINBOARD_CARDS_V1
        || partition_total_v1(&input.current_configuration.sideboard)? > MAX_SIDEBOARD_CARDS_V1
    {
        return Err("native sideboard player-visible deck size rules are invalid".to_owned());
    }
    validate_configuration_v1(&input.current_configuration)?;
    Ok(())
}

pub fn competitive_native_sideboard_model_input_commitment_v1(
    input: &MtgoCompetitiveNativeSideboardModelInputV1,
) -> Result<String, String> {
    validate_competitive_native_sideboard_model_input_v1(input)?;
    canonical_commitment_v1(NATIVE_SIDEBOARD_MODEL_INPUT_DOMAIN_V1, input)
}

pub fn validate_competitive_native_sideboard_model_selection_v1(
    input: &MtgoCompetitiveNativeSideboardModelInputV1,
    selection: &MtgoCompetitiveNativeSideboardModelSelectionV1,
) -> Result<(), String> {
    validate_competitive_native_sideboard_model_input_v1(input)?;
    validate_configuration_v1(&selection.target_configuration)?;
    if partition_total_v1(&selection.target_configuration.mainboard)? < MIN_MAINBOARD_CARDS_V1
        || partition_total_v1(&selection.target_configuration.sideboard)? > MAX_SIDEBOARD_CARDS_V1
    {
        return Err("native sideboard selection violates the visible deck size rules".to_owned());
    }
    if combined_inventory_v1(&input.current_configuration)?
        != combined_inventory_v1(&selection.target_configuration)?
    {
        return Err(
            "native sideboard selection changed the player-visible card inventory".to_owned(),
        );
    }
    Ok(())
}

pub fn competitive_native_sideboard_model_selection_commitment_v1(
    input: &MtgoCompetitiveNativeSideboardModelInputV1,
    selection: &MtgoCompetitiveNativeSideboardModelSelectionV1,
) -> Result<String, String> {
    validate_competitive_native_sideboard_model_selection_v1(input, selection)?;
    let input_commitment = competitive_native_sideboard_model_input_commitment_v1(input)?;
    let canonical = serde_json::to_vec(selection)
        .map_err(|error| format!("serialize native sideboard model selection: {error}"))?;
    Ok(commitment_parts_v1(
        NATIVE_SIDEBOARD_MODEL_SELECTION_DOMAIN_V1,
        &[
            input_commitment.as_bytes(),
            &canonical,
            b"player_visible_target_configuration_no_adapter_authority_no_input_no_submit",
        ],
    ))
}

fn validate_configuration_v1(
    configuration: &MtgoCompetitiveNativeSideboardConfigurationV1,
) -> Result<(), String> {
    if configuration.mainboard.is_empty()
        || configuration.mainboard.len() > MAX_CARD_KINDS_V1
        || configuration.sideboard.len() > MAX_CARD_KINDS_V1
    {
        return Err("native sideboard configuration has an invalid partition size".to_owned());
    }
    let mainboard_total = validate_partition_v1(&configuration.mainboard, "mainboard")?;
    let sideboard_total = validate_partition_v1(&configuration.sideboard, "sideboard")?;
    if mainboard_total
        .checked_add(sideboard_total)
        .filter(|total| *total <= MAX_TOTAL_CARDS_V1)
        .is_none()
    {
        return Err("native sideboard configuration total is invalid".to_owned());
    }
    Ok(())
}

fn partition_total_v1(cards: &[MtgoCompetitiveNativeSideboardCardCountV1]) -> Result<u32, String> {
    cards.iter().try_fold(0_u32, |total, card| {
        total
            .checked_add(u32::from(card.count))
            .ok_or_else(|| "native sideboard partition count overflow".to_owned())
    })
}

fn validate_partition_v1(
    cards: &[MtgoCompetitiveNativeSideboardCardCountV1],
    label: &str,
) -> Result<u32, String> {
    let mut prior_name: Option<&str> = None;
    let mut names = HashSet::new();
    let mut total = 0_u32;
    for card in cards {
        let name = card.visible_card_name.as_str();
        if name.trim().is_empty()
            || name.len() > 256
            || name.chars().any(char::is_control)
            || card.count == 0
            || prior_name.is_some_and(|prior| name <= prior)
            || !names.insert(name)
        {
            return Err(format!(
                "native sideboard {label} visible-card identity or order is invalid"
            ));
        }
        total = total
            .checked_add(u32::from(card.count))
            .ok_or_else(|| format!("native sideboard {label} count overflow"))?;
        prior_name = Some(name);
    }
    Ok(total)
}

fn combined_inventory_v1(
    configuration: &MtgoCompetitiveNativeSideboardConfigurationV1,
) -> Result<BTreeMap<&str, u32>, String> {
    let mut inventory = BTreeMap::new();
    for card in configuration
        .mainboard
        .iter()
        .chain(&configuration.sideboard)
    {
        let count = inventory
            .entry(card.visible_card_name.as_str())
            .or_insert(0_u32);
        *count = count
            .checked_add(u32::from(card.count))
            .ok_or("native sideboard combined inventory overflow")?;
    }
    Ok(inventory)
}

fn canonical_commitment_v1<T: Serialize>(domain: &[u8], value: &T) -> Result<String, String> {
    let canonical = serde_json::to_vec(value)
        .map_err(|error| format!("serialize native sideboard model value: {error}"))?;
    Ok(commitment_parts_v1(
        domain,
        &[
            &canonical,
            b"player_visible_game_information_only_no_model_execution_no_input",
        ],
    ))
}

fn commitment_parts_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        u64::try_from(domain.len())
            .expect("static commitment domain length fits u64")
            .to_be_bytes(),
    );
    hasher.update(domain);
    for part in parts {
        hasher.update(
            u64::try_from(part.len())
                .expect("in-memory commitment part length fits u64")
                .to_be_bytes(),
        );
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1;

    fn exact_configuration_v1() -> MtgoCompetitiveDeckConfigurationV1 {
        MtgoCompetitiveDeckConfigurationV1 {
            mainboard: vec![
                MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: 9,
                    card_name: "Mountain".to_owned(),
                    count: 56,
                },
                MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: 3,
                    card_name: "Lightning Bolt".to_owned(),
                    count: 4,
                },
            ],
            sideboard: vec![MtgoCompetitiveDeckCardCountV1 {
                card_db_id: 3,
                card_name: "Lightning Bolt".to_owned(),
                count: 15,
            }],
        }
    }

    fn input_v1(next_game_number: u8) -> MtgoCompetitiveNativeSideboardModelInputV1 {
        let (acting_player_games_won, opponent_games_won) = if next_game_number == 2 {
            (1, 0)
        } else {
            (1, 1)
        };
        MtgoCompetitiveNativeSideboardModelInputV1 {
            next_game_number,
            acting_player_games_won,
            opponent_games_won,
            current_configuration: visible_native_sideboard_configuration_v1(
                &exact_configuration_v1(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn visible_configuration_strips_adapter_ids_and_canonicalizes_names() {
        let input = input_v1(2);
        validate_competitive_native_sideboard_model_input_v1(&input).unwrap();
        assert_eq!(
            input.current_configuration.mainboard[0].visible_card_name,
            "Lightning Bolt"
        );
        let serialized = serde_json::to_string(&input).unwrap();
        for forbidden in [
            "card_db_id",
            "sha256",
            "schema_version",
            "complete",
            "minimum_mainboard_count",
            "maximum_sideboard_count",
            "event",
            "match_identity",
            "authorization",
            "capture",
            "classifier",
            "deck_manifest",
            "policy_deployment",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn only_visible_best_of_three_sideboard_scores_validate() {
        validate_competitive_native_sideboard_model_input_v1(&input_v1(2)).unwrap();
        validate_competitive_native_sideboard_model_input_v1(&input_v1(3)).unwrap();
        for (game, acting, opponent) in [(1, 0, 0), (2, 0, 0), (2, 1, 1), (3, 2, 0)] {
            let mut invalid = input_v1(2);
            invalid.next_game_number = game;
            invalid.acting_player_games_won = acting;
            invalid.opponent_games_won = opponent;
            assert!(validate_competitive_native_sideboard_model_input_v1(&invalid).is_err());
        }
    }

    #[test]
    fn unchanged_and_inventory_conserving_selections_validate() {
        let input = input_v1(2);
        let unchanged = MtgoCompetitiveNativeSideboardModelSelectionV1 {
            target_configuration: input.current_configuration.clone(),
        };
        validate_competitive_native_sideboard_model_selection_v1(&input, &unchanged).unwrap();

        let mut changed = unchanged.clone();
        changed.target_configuration.mainboard[0].count += 1;
        changed.target_configuration.sideboard[0].count += 1;
        assert!(
            validate_competitive_native_sideboard_model_selection_v1(&input, &changed).is_err()
        );

        let mut conserved = unchanged.clone();
        conserved.target_configuration.mainboard[0].count += 1;
        conserved.target_configuration.sideboard[0].count -= 1;
        validate_competitive_native_sideboard_model_selection_v1(&input, &conserved).unwrap();
        assert_ne!(
            competitive_native_sideboard_model_selection_commitment_v1(&input, &unchanged).unwrap(),
            competitive_native_sideboard_model_selection_commitment_v1(&input, &conserved).unwrap()
        );
    }

    #[test]
    fn commitments_bind_visible_semantics_and_invalid_names_fail() {
        let input = input_v1(2);
        let original = competitive_native_sideboard_model_input_commitment_v1(&input).unwrap();
        let mut changed = input.clone();
        changed.current_configuration.mainboard[0].count += 1;
        changed.current_configuration.sideboard[0].count -= 1;
        assert_ne!(
            competitive_native_sideboard_model_input_commitment_v1(&changed).unwrap(),
            original
        );
        changed.current_configuration.mainboard[0].visible_card_name = "bad\nname".to_owned();
        assert!(validate_competitive_native_sideboard_model_input_v1(&changed).is_err());
    }
}
