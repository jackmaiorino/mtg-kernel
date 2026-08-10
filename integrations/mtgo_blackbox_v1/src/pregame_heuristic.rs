use crate::{
    resolve_checked_untrusted_kernel_card_correspondence_v1,
    CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1, MtgoContractErrorV1,
    MtgoKernelCardCorrespondenceDispositionV1, MtgoOfflineVisibleCardIdentityClassificationV1,
    MtgoPregameActionSemanticV1,
};
use mtg_kernel::card_def::{Supertype, CARD_DEFS, KERNEL_CARDDB_HASH};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

pub const MTGO_PREGAME_HEURISTIC_DECK_SCHEMA_V1: u32 = 1;

const PREGAME_HEURISTIC_DECK_DOMAIN_V1: &[u8] = b"mtgo-pregame-heuristic-deck-v1";
const PREGAME_HEURISTIC_SELECTION_DOMAIN_V1: &[u8] = b"mtgo-pregame-land-window-selection-v1";
const EXPECTED_DECK_CARD_COUNT_V1: u16 = 60;
const VISIBLE_MULLIGAN_HAND_COUNT_V1: usize = 7;
const MAX_DECK_ID_BYTES_V1: usize = 128;
const MAX_DISTINCT_DECK_CARDS_V1: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPregameHeuristicDeckCardV1 {
    pub visible_card_name: String,
    pub count: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPregameHeuristicDeckV1 {
    pub schema_version: u32,
    pub deck_id: String,
    pub cards: Vec<MtgoPregameHeuristicDeckCardV1>,
}

struct CheckedDeckCardV1 {
    count: u16,
    is_land: bool,
    card_db_id: u16,
    correspondence_commitment_sha256: String,
}

/// Caller-supplied deck knowledge checked against the compile-bound kernel.
///
/// This does not prove that MTGO loaded the described deck. It is suitable
/// only as an offline heuristic input and grants no observation, model, or
/// input authority.
pub struct CheckedUntrustedMtgoPregameHeuristicDeckV1 {
    deck_id: String,
    cards: BTreeMap<String, CheckedDeckCardV1>,
    land_count: u16,
    nonland_count: u16,
    deck_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPregameHeuristicDeckV1 {
    pub fn deck_id(&self) -> &str {
        &self.deck_id
    }

    pub fn land_count(&self) -> u16 {
        self.land_count
    }

    pub fn nonland_count(&self) -> u16 {
        self.nonland_count
    }

    pub fn deck_commitment_sha256(&self) -> &str {
        &self.deck_commitment_sha256
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPregameHeuristicReasonV1 {
    AllLandDeckNoRedrawImprovement,
    KeepableLandWindow,
    MinimumKeepSize,
    NoKeepableLandWindow,
}

/// Offline-only, explicitly non-model pregame selection.
///
/// The selected semantic is bound to the exact visible-identity candidate and
/// checked deck description. No coordinates, controls, actuator intent, or
/// authority are exposed.
pub struct CheckedUntrustedMtgoPregameHeuristicSelectionV1 {
    source_identity_candidate_commitment_sha256: String,
    deck_commitment_sha256: String,
    prospective_keep_size: u8,
    visible_land_count: u8,
    visible_nonland_count: u8,
    feasible_kept_land_min: u8,
    feasible_kept_land_max: u8,
    selected_action: MtgoPregameActionSemanticV1,
    reason: MtgoPregameHeuristicReasonV1,
    selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPregameHeuristicSelectionV1 {
    pub fn source_identity_candidate_commitment_sha256(&self) -> &str {
        &self.source_identity_candidate_commitment_sha256
    }

    pub fn deck_commitment_sha256(&self) -> &str {
        &self.deck_commitment_sha256
    }

    pub fn prospective_keep_size(&self) -> u8 {
        self.prospective_keep_size
    }

    pub fn visible_land_count(&self) -> u8 {
        self.visible_land_count
    }

    pub fn visible_nonland_count(&self) -> u8 {
        self.visible_nonland_count
    }

    pub fn feasible_kept_land_min(&self) -> u8 {
        self.feasible_kept_land_min
    }

    pub fn feasible_kept_land_max(&self) -> u8 {
        self.feasible_kept_land_max
    }

    pub fn selected_action(&self) -> &MtgoPregameActionSemanticV1 {
        &self.selected_action
    }

    pub fn reason(&self) -> MtgoPregameHeuristicReasonV1 {
        self.reason
    }

    pub fn selection_commitment_sha256(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn is_model_decision(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn check_untrusted_mtgo_pregame_heuristic_deck_v1(
    deck: MtgoPregameHeuristicDeckV1,
) -> Result<CheckedUntrustedMtgoPregameHeuristicDeckV1, MtgoContractErrorV1> {
    if deck.schema_version != MTGO_PREGAME_HEURISTIC_DECK_SCHEMA_V1 {
        return Err(error_v1(
            "pregame_heuristic_deck_schema",
            deck.schema_version.to_string(),
        ));
    }
    validate_identifier_v1("pregame_heuristic_deck_id", &deck.deck_id)?;
    if deck.cards.is_empty() || deck.cards.len() > MAX_DISTINCT_DECK_CARDS_V1 {
        return Err(error_v1(
            "pregame_heuristic_deck_distinct_count",
            deck.cards.len().to_string(),
        ));
    }

    let mut names = HashSet::with_capacity(deck.cards.len());
    let mut checked_cards = BTreeMap::new();
    let mut total_count = 0_u16;
    let mut land_count = 0_u16;
    for card in deck.cards {
        if card.count == 0 || !names.insert(card.visible_card_name.clone()) {
            return Err(error_v1(
                "pregame_heuristic_deck_card_shape",
                card.visible_card_name,
            ));
        }
        let correspondence =
            resolve_checked_untrusted_kernel_card_correspondence_v1(&card.visible_card_name)?;
        if correspondence.disposition()
            != MtgoKernelCardCorrespondenceDispositionV1::FullySupportedDeckCard
        {
            return Err(error_v1(
                "pregame_heuristic_deck_card_not_deck_card",
                card.visible_card_name,
            ));
        }
        let definition = CARD_DEFS
            .get(usize::from(correspondence.card_db_id()))
            .ok_or_else(|| {
                error_v1(
                    "pregame_heuristic_deck_card_definition",
                    card.visible_card_name.as_str(),
                )
            })?;
        if card.count > 4 && !definition.supertypes.contains(&Supertype::Basic) {
            return Err(error_v1(
                "pregame_heuristic_deck_copy_limit",
                card.visible_card_name,
            ));
        }
        total_count = total_count.checked_add(card.count).ok_or_else(|| {
            error_v1(
                "pregame_heuristic_deck_total_overflow",
                card.count.to_string(),
            )
        })?;
        if definition.is_land {
            land_count = land_count.checked_add(card.count).ok_or_else(|| {
                error_v1(
                    "pregame_heuristic_deck_land_overflow",
                    card.count.to_string(),
                )
            })?;
        }
        checked_cards.insert(
            card.visible_card_name,
            CheckedDeckCardV1 {
                count: card.count,
                is_land: definition.is_land,
                card_db_id: correspondence.card_db_id(),
                correspondence_commitment_sha256: correspondence
                    .correspondence_commitment_sha256()
                    .to_owned(),
            },
        );
    }
    if total_count != EXPECTED_DECK_CARD_COUNT_V1 {
        return Err(error_v1(
            "pregame_heuristic_deck_total",
            total_count.to_string(),
        ));
    }
    let nonland_count = total_count - land_count;
    let deck_commitment_sha256 = deck_commitment_v1(&deck.deck_id, &checked_cards);
    Ok(CheckedUntrustedMtgoPregameHeuristicDeckV1 {
        deck_id: deck.deck_id,
        cards: checked_cards,
        land_count,
        nonland_count,
        deck_commitment_sha256,
    })
}

pub fn select_checked_untrusted_mtgo_pregame_heuristic_v1(
    identity: &CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1,
    deck: &CheckedUntrustedMtgoPregameHeuristicDeckV1,
) -> Result<CheckedUntrustedMtgoPregameHeuristicSelectionV1, MtgoContractErrorV1> {
    if identity.classification() != MtgoOfflineVisibleCardIdentityClassificationV1::Match
        || identity.visible_hand_count() != Some(VISIBLE_MULLIGAN_HAND_COUNT_V1 as u8)
        || identity.matched_identity_count() != VISIBLE_MULLIGAN_HAND_COUNT_V1 as u8
        || identity.identities().len() != VISIBLE_MULLIGAN_HAND_COUNT_V1
    {
        return Err(error_v1(
            "pregame_heuristic_identity_incomplete",
            "a complete seven-card visible identity result is required",
        ));
    }
    let prospective_keep_size = identity.prospective_keep_size().ok_or_else(|| {
        error_v1(
            "pregame_heuristic_keep_size_missing",
            "mulligan prompt did not expose a keep size",
        )
    })?;
    if !(1..=7).contains(&prospective_keep_size) {
        return Err(error_v1(
            "pregame_heuristic_keep_size_invalid",
            prospective_keep_size.to_string(),
        ));
    }

    let mut observed_counts = BTreeMap::<&str, u16>::new();
    let mut visible_land_count = 0_u8;
    for (expected_ordinal, card) in identity.identities().iter().enumerate() {
        if card.ordinal() != u8::try_from(expected_ordinal).unwrap_or(u8::MAX) {
            return Err(error_v1(
                "pregame_heuristic_identity_ordinal",
                card.ordinal().to_string(),
            ));
        }
        let deck_card = deck.cards.get(card.visible_card_name()).ok_or_else(|| {
            error_v1(
                "pregame_heuristic_card_absent_from_deck",
                card.visible_card_name(),
            )
        })?;
        let observed = observed_counts.entry(card.visible_card_name()).or_default();
        *observed += 1;
        if *observed > deck_card.count {
            return Err(error_v1(
                "pregame_heuristic_card_count_exceeds_deck",
                card.visible_card_name(),
            ));
        }
        if deck_card.is_land {
            visible_land_count += 1;
        }
    }
    let visible_nonland_count = VISIBLE_MULLIGAN_HAND_COUNT_V1 as u8 - visible_land_count;
    let feasible_kept_land_min = prospective_keep_size.saturating_sub(visible_nonland_count);
    let feasible_kept_land_max = prospective_keep_size.min(visible_land_count);
    let (selected_action, reason) = if prospective_keep_size == 1 {
        (
            MtgoPregameActionSemanticV1::KeepOpeningHand,
            MtgoPregameHeuristicReasonV1::MinimumKeepSize,
        )
    } else if deck.nonland_count == 0 {
        (
            MtgoPregameActionSemanticV1::KeepOpeningHand,
            MtgoPregameHeuristicReasonV1::AllLandDeckNoRedrawImprovement,
        )
    } else {
        let (target_min, target_max) = target_kept_land_window_v1(prospective_keep_size);
        let overlap_min = feasible_kept_land_min.max(target_min);
        let overlap_max = feasible_kept_land_max.min(target_max);
        if overlap_min <= overlap_max {
            (
                MtgoPregameActionSemanticV1::KeepOpeningHand,
                MtgoPregameHeuristicReasonV1::KeepableLandWindow,
            )
        } else {
            (
                MtgoPregameActionSemanticV1::Mulligan {
                    next_hand_size: prospective_keep_size - 1,
                },
                MtgoPregameHeuristicReasonV1::NoKeepableLandWindow,
            )
        }
    };
    let selection_commitment_sha256 = selection_commitment_v1(
        identity.candidate_commitment_sha256(),
        deck.deck_commitment_sha256(),
        prospective_keep_size,
        visible_land_count,
        visible_nonland_count,
        feasible_kept_land_min,
        feasible_kept_land_max,
        &selected_action,
        reason,
    );

    Ok(CheckedUntrustedMtgoPregameHeuristicSelectionV1 {
        source_identity_candidate_commitment_sha256: identity
            .candidate_commitment_sha256()
            .to_owned(),
        deck_commitment_sha256: deck.deck_commitment_sha256().to_owned(),
        prospective_keep_size,
        visible_land_count,
        visible_nonland_count,
        feasible_kept_land_min,
        feasible_kept_land_max,
        selected_action,
        reason,
        selection_commitment_sha256,
    })
}

fn target_kept_land_window_v1(keep_size: u8) -> (u8, u8) {
    match keep_size {
        7 => (2, 5),
        6 | 5 => (2, 4),
        4 => (1, 3),
        3 | 2 => (1, 2),
        1 => (0, 1),
        _ => (u8::MAX, 0),
    }
}

fn deck_commitment_v1(deck_id: &str, cards: &BTreeMap<String, CheckedDeckCardV1>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(PREGAME_HEURISTIC_DECK_DOMAIN_V1);
    update_hash_part_v1(&mut hasher, &KERNEL_CARDDB_HASH.to_le_bytes());
    update_hash_part_v1(&mut hasher, deck_id.as_bytes());
    update_hash_part_v1(&mut hasher, &(cards.len() as u64).to_le_bytes());
    for (name, card) in cards {
        update_hash_part_v1(&mut hasher, name.as_bytes());
        update_hash_part_v1(&mut hasher, &card.count.to_le_bytes());
        update_hash_part_v1(&mut hasher, &[u8::from(card.is_land)]);
        update_hash_part_v1(&mut hasher, &card.card_db_id.to_le_bytes());
        update_hash_part_v1(
            &mut hasher,
            card.correspondence_commitment_sha256.as_bytes(),
        );
    }
    format!("{:x}", hasher.finalize())
}

#[allow(clippy::too_many_arguments)]
fn selection_commitment_v1(
    source_identity_candidate_commitment_sha256: &str,
    deck_commitment_sha256: &str,
    prospective_keep_size: u8,
    visible_land_count: u8,
    visible_nonland_count: u8,
    feasible_kept_land_min: u8,
    feasible_kept_land_max: u8,
    selected_action: &MtgoPregameActionSemanticV1,
    reason: MtgoPregameHeuristicReasonV1,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(PREGAME_HEURISTIC_SELECTION_DOMAIN_V1);
    update_hash_part_v1(
        &mut hasher,
        source_identity_candidate_commitment_sha256.as_bytes(),
    );
    update_hash_part_v1(&mut hasher, deck_commitment_sha256.as_bytes());
    update_hash_part_v1(&mut hasher, &[prospective_keep_size]);
    update_hash_part_v1(&mut hasher, &[visible_land_count]);
    update_hash_part_v1(&mut hasher, &[visible_nonland_count]);
    update_hash_part_v1(&mut hasher, &[feasible_kept_land_min]);
    update_hash_part_v1(&mut hasher, &[feasible_kept_land_max]);
    match selected_action {
        MtgoPregameActionSemanticV1::KeepOpeningHand => {
            update_hash_part_v1(&mut hasher, b"keep_opening_hand")
        }
        MtgoPregameActionSemanticV1::Mulligan { next_hand_size } => {
            update_hash_part_v1(&mut hasher, b"mulligan");
            update_hash_part_v1(&mut hasher, &[*next_hand_size]);
        }
    }
    update_hash_part_v1(
        &mut hasher,
        &[match reason {
            MtgoPregameHeuristicReasonV1::AllLandDeckNoRedrawImprovement => 0,
            MtgoPregameHeuristicReasonV1::KeepableLandWindow => 1,
            MtgoPregameHeuristicReasonV1::MinimumKeepSize => 2,
            MtgoPregameHeuristicReasonV1::NoKeepableLandWindow => 3,
        }],
    );
    format!("{:x}", hasher.finalize())
}

fn update_hash_part_v1(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn validate_identifier_v1(field: &'static str, value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > MAX_DECK_ID_BYTES_V1
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(error_v1(field, value));
    }
    Ok(())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline_visible_card_identity::mock_complete_mulligan_identity_candidate_v1;

    fn all_land_deck_v1() -> CheckedUntrustedMtgoPregameHeuristicDeckV1 {
        check_untrusted_mtgo_pregame_heuristic_deck_v1(MtgoPregameHeuristicDeckV1 {
            schema_version: 1,
            deck_id: "kernel-basics-v1".to_owned(),
            cards: vec![
                MtgoPregameHeuristicDeckCardV1 {
                    visible_card_name: "Forest".to_owned(),
                    count: 30,
                },
                MtgoPregameHeuristicDeckCardV1 {
                    visible_card_name: "Island".to_owned(),
                    count: 30,
                },
            ],
        })
        .unwrap()
    }

    fn burn_shape_deck_v1() -> CheckedUntrustedMtgoPregameHeuristicDeckV1 {
        let mut counts = BTreeMap::<String, u16>::new();
        for card_id in mtg_kernel::runtime_decks::runtime_deck_by_id("Burn")
            .unwrap()
            .card_ids
        {
            *counts
                .entry(CARD_DEFS[usize::from(*card_id)].name.to_owned())
                .or_default() += 1;
        }
        check_untrusted_mtgo_pregame_heuristic_deck_v1(MtgoPregameHeuristicDeckV1 {
            schema_version: 1,
            deck_id: "kernel-runtime-burn-v1".to_owned(),
            cards: counts
                .into_iter()
                .map(
                    |(visible_card_name, count)| MtgoPregameHeuristicDeckCardV1 {
                        visible_card_name,
                        count,
                    },
                )
                .collect(),
        })
        .unwrap()
    }

    fn hand_with_land_count_v1(
        deck: &CheckedUntrustedMtgoPregameHeuristicDeckV1,
        desired_land_count: usize,
    ) -> Vec<String> {
        let mut names = Vec::with_capacity(VISIBLE_MULLIGAN_HAND_COUNT_V1);
        for (name, card) in deck.cards.iter().filter(|(_, card)| card.is_land) {
            for _ in 0..usize::from(card.count) {
                if names.len() == desired_land_count {
                    break;
                }
                names.push(name.clone());
            }
        }
        assert_eq!(names.len(), desired_land_count);
        for (name, card) in deck.cards.iter().filter(|(_, card)| !card.is_land) {
            for _ in 0..usize::from(card.count) {
                if names.len() == VISIBLE_MULLIGAN_HAND_COUNT_V1 {
                    break;
                }
                names.push(name.clone());
            }
        }
        assert_eq!(names.len(), VISIBLE_MULLIGAN_HAND_COUNT_V1);
        names
    }

    #[test]
    fn all_land_calibration_deck_keeps_because_redraw_cannot_improve_shape() {
        let deck = all_land_deck_v1();
        let identity = mock_complete_mulligan_identity_candidate_v1(
            &[
                "Forest", "Island", "Forest", "Forest", "Forest", "Forest", "Island",
            ],
            7,
        );
        let selection =
            select_checked_untrusted_mtgo_pregame_heuristic_v1(&identity, &deck).unwrap();

        assert_eq!(
            selection.selected_action(),
            &MtgoPregameActionSemanticV1::KeepOpeningHand
        );
        assert_eq!(selection.visible_land_count(), 7);
        assert_eq!(
            selection.reason(),
            MtgoPregameHeuristicReasonV1::AllLandDeckNoRedrawImprovement
        );
        assert!(!selection.is_model_decision());
        assert!(!selection.safe_for_input());
    }

    #[test]
    fn land_window_keeps_or_mulligans_with_london_bottoming_feasibility() {
        let deck = burn_shape_deck_v1();
        let keep_names = hand_with_land_count_v1(&deck, 3);
        let keep_refs = keep_names.iter().map(String::as_str).collect::<Vec<_>>();
        let keep = mock_complete_mulligan_identity_candidate_v1(&keep_refs, 7);
        let keep = select_checked_untrusted_mtgo_pregame_heuristic_v1(&keep, &deck).unwrap();
        assert_eq!(
            keep.selected_action(),
            &MtgoPregameActionSemanticV1::KeepOpeningHand
        );
        assert_eq!(
            keep.reason(),
            MtgoPregameHeuristicReasonV1::KeepableLandWindow
        );

        let mulligan_names = hand_with_land_count_v1(&deck, 0);
        let mulligan_refs = mulligan_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mulligan = mock_complete_mulligan_identity_candidate_v1(&mulligan_refs, 7);
        let mulligan =
            select_checked_untrusted_mtgo_pregame_heuristic_v1(&mulligan, &deck).unwrap();
        assert_eq!(
            mulligan.selected_action(),
            &MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 }
        );
        assert_eq!(
            mulligan.reason(),
            MtgoPregameHeuristicReasonV1::NoKeepableLandWindow
        );
    }

    #[test]
    fn minimum_size_keeps_and_source_or_deck_mismatch_fails_closed() {
        let deck = burn_shape_deck_v1();
        let minimum_names = hand_with_land_count_v1(&deck, 0);
        let minimum_refs = minimum_names.iter().map(String::as_str).collect::<Vec<_>>();
        let minimum = mock_complete_mulligan_identity_candidate_v1(&minimum_refs, 1);
        let minimum = select_checked_untrusted_mtgo_pregame_heuristic_v1(&minimum, &deck).unwrap();
        assert_eq!(
            minimum.selected_action(),
            &MtgoPregameActionSemanticV1::KeepOpeningHand
        );
        assert_eq!(
            minimum.reason(),
            MtgoPregameHeuristicReasonV1::MinimumKeepSize
        );

        let wrong_deck = mock_complete_mulligan_identity_candidate_v1(
            &[
                "Island", "Island", "Island", "Island", "Island", "Island", "Island",
            ],
            7,
        );
        assert_eq!(
            select_checked_untrusted_mtgo_pregame_heuristic_v1(&wrong_deck, &deck)
                .err()
                .unwrap()
                .code(),
            "pregame_heuristic_card_absent_from_deck"
        );
    }

    #[test]
    fn deck_json_and_counts_are_strict() {
        let unknown = r#"{
            "schema_version":1,
            "deck_id":"x",
            "cards":[{"visible_card_name":"Island","count":60}],
            "coordinates":[1,2]
        }"#;
        assert!(serde_json::from_str::<MtgoPregameHeuristicDeckV1>(unknown).is_err());

        let duplicate = MtgoPregameHeuristicDeckV1 {
            schema_version: 1,
            deck_id: "duplicate-v1".to_owned(),
            cards: vec![
                MtgoPregameHeuristicDeckCardV1 {
                    visible_card_name: "Island".to_owned(),
                    count: 30,
                },
                MtgoPregameHeuristicDeckCardV1 {
                    visible_card_name: "Island".to_owned(),
                    count: 30,
                },
            ],
        };
        assert_eq!(
            check_untrusted_mtgo_pregame_heuristic_deck_v1(duplicate)
                .err()
                .unwrap()
                .code(),
            "pregame_heuristic_deck_card_shape"
        );

        let too_many_nonbasics = MtgoPregameHeuristicDeckV1 {
            schema_version: 1,
            deck_id: "copy-limit-v1".to_owned(),
            cards: vec![
                MtgoPregameHeuristicDeckCardV1 {
                    visible_card_name: "Mountain".to_owned(),
                    count: 55,
                },
                MtgoPregameHeuristicDeckCardV1 {
                    visible_card_name: "Lightning Bolt".to_owned(),
                    count: 5,
                },
            ],
        };
        assert_eq!(
            check_untrusted_mtgo_pregame_heuristic_deck_v1(too_many_nonbasics)
                .err()
                .unwrap()
                .code(),
            "pregame_heuristic_deck_copy_limit"
        );
    }
}
