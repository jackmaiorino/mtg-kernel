//! Explicit own selected-mainboard context. No session or opponent lookup.
use crate::card_def::CARD_DEFS;
use crate::rl::PlayerSeatV1;
use crate::sideboard::{mainboard_slice_sha256_v1, DeckConfigurationV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub const WIDTH: usize = 192;
pub const SCHEMA: &str = "mtg-kernel-own-registration/v1";
pub const NORMALIZATION: &str = "selected-mainboard-count-div-positive-size/f32/v1";
pub const REGISTRY_SHA256: &str =
    "ef738001c373076039f211582f5fd07b11903788e70641ec7e23c84ba4f69b14";
pub const ROSTER_SHA256: &str = "bb834d0ba9a0b296f90dbcc489532643ca60cee54354b99eeaf169a08e00100d";
pub const ROSTER: [&str; 9] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries",
];
const REGISTRY: &[u8] = include_bytes!("../../data/cards_v1.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationModeV1 {
    Own,
    Mean,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationContractV1 {
    pub schema: String,
    pub registry_sha256: String,
    pub contract_sha256: String,
    pub roster_sha256: String,
    pub mean_sha256: String,
    pub normalization: String,
    pub width: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct RegistrationExpectationV1<'a> {
    pub external_episode_id: &'a str,
    pub actor: PlayerSeatV1,
    pub selected_mainboard_sha256: [u8; 32],
}

/// Construction requires the caller's validated selected configuration. Fields
/// stay private so later calls cannot silently replace a player's registration.
#[derive(Clone, Debug)]
pub struct KnownOwnMainboardV1 {
    rows: RegistrationFeatureRowsV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationFeatureRowsV1 {
    pub schema: String,
    pub registry_sha256: String,
    pub contract_sha256: String,
    pub roster_sha256: String,
    pub mode: RegistrationModeV1,
    pub external_episode_id: String,
    pub actor: PlayerSeatV1,
    pub selected_mainboard_sha256: [u8; 32],
    pub mainboard_size: u32,
    pub counts: Vec<u32>,
    pub features: Vec<f32>,
}

fn counts_v1(cards: &[u16]) -> Result<Vec<u32>, String> {
    if cards.is_empty() {
        return Err("own mainboard must have positive size".into());
    }
    let mut counts = vec![0u32; WIDTH];
    for &card in cards {
        let count = counts
            .get_mut(usize::from(card))
            .ok_or("own mainboard card outside pinned registry")?;
        *count = count.checked_add(1).ok_or("own mainboard count overflow")?;
    }
    Ok(counts)
}

fn normalized_v1(counts: &[u32], size: u32) -> Vec<f32> {
    counts
        .iter()
        .map(|&count| count as f32 / size as f32)
        .collect()
}

struct Catalog {
    contract: RegistrationContractV1,
    mean: Vec<f32>,
}
fn catalog_v1() -> Result<&'static Catalog, String> {
    static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        if CARD_DEFS.len() != WIDTH || format!("{:x}", Sha256::digest(REGISTRY)) != REGISTRY_SHA256 {
            return Err("own registration pinned registry differs".into());
        }
        let registry: serde_json::Value = serde_json::from_slice(REGISTRY).map_err(|e| e.to_string())?;
        let cards = registry["cards"].as_array().ok_or("own registration registry missing cards")?;
        if cards.len() != WIDTH || cards.iter().zip(CARD_DEFS.iter()).any(|(raw, card)| raw["name"].as_str() != Some(card.name)) {
            return Err("own registration compiled registry order differs".into());
        }
        let mut hash = Sha256::new();
        hash.update(b"own-registration-roster/v1\0");
        let mut mean = vec![0.0f32; WIDTH];
        for name in ROSTER {
            let deck = crate::runtime_decks::runtime_deck_by_id(name).ok_or("own registration roster deck missing")?;
            if !deck.is_fully_materialized() || deck.mainboard_count != 60 { return Err("own registration roster shape differs".into()); }
            let counts = counts_v1(deck.card_ids)?;
            hash.update((name.len() as u32).to_le_bytes()); hash.update(name.as_bytes());
            hash.update((deck.mainboard_count as u32).to_le_bytes());
            for count in &counts { hash.update(count.to_le_bytes()); }
            // This ordered f32 sum and final division are part of the contract.
            for (value, normalized) in mean.iter_mut().zip(normalized_v1(&counts, deck.mainboard_count as u32)) { *value += normalized; }
        }
        for value in &mut mean { *value /= ROSTER.len() as f32; }
        let roster_sha256 = format!("{:x}", hash.finalize());
        if roster_sha256 != ROSTER_SHA256 { return Err("own registration pinned roster differs".into()); }
        let mut hash = Sha256::new();
        for value in &mean { hash.update(value.to_bits().to_le_bytes()); }
        let mean_sha256 = format!("{:x}", hash.finalize());
        let descriptor = format!("{SCHEMA}\n{REGISTRY_SHA256}\n{ROSTER_SHA256}\n{NORMALIZATION}\n{WIDTH}\n{mean_sha256}\ncolumn=zero-based-card-id;mean=ordered-f32-sum-then-divide-nine;projection=64x192-bias-free-state-preactivation\n");
        let contract = RegistrationContractV1 {
            schema: SCHEMA.into(), registry_sha256: REGISTRY_SHA256.into(),
            contract_sha256: format!("{:x}", Sha256::digest(descriptor.as_bytes())),
            roster_sha256, mean_sha256, normalization: NORMALIZATION.into(), width: WIDTH,
        };
        Ok(Catalog { contract, mean })
    }).as_ref().map_err(Clone::clone)
}

pub fn contract_identity_v1() -> Result<RegistrationContractV1, String> {
    Ok(catalog_v1()?.contract.clone())
}
pub fn fixed_mean_features_v1() -> Result<Vec<f32>, String> {
    Ok(catalog_v1()?.mean.clone())
}

impl KnownOwnMainboardV1 {
    pub fn new(
        external_episode_id: impl Into<String>,
        actor: PlayerSeatV1,
        selected: &DeckConfigurationV1,
    ) -> Result<Self, String> {
        let contract = contract_identity_v1()?;
        let counts = counts_v1(selected.mainboard())?;
        let mainboard_size =
            u32::try_from(selected.mainboard().len()).map_err(|_| "own mainboard size overflow")?;
        let rows = RegistrationFeatureRowsV1 {
            schema: SCHEMA.into(),
            registry_sha256: contract.registry_sha256,
            contract_sha256: contract.contract_sha256,
            roster_sha256: contract.roster_sha256,
            mode: RegistrationModeV1::Own,
            external_episode_id: external_episode_id.into(),
            actor,
            selected_mainboard_sha256: selected.mainboard_sha256_v1(),
            mainboard_size,
            features: normalized_v1(&counts, mainboard_size),
            counts,
        };
        rows.validate()?;
        Ok(Self { rows })
    }
    pub fn rows(&self, mode: RegistrationModeV1) -> Result<RegistrationFeatureRowsV1, String> {
        let mut rows = self.rows.clone();
        rows.mode = mode;
        if mode == RegistrationModeV1::Mean {
            rows.features = fixed_mean_features_v1()?;
        }
        rows.validate()?;
        Ok(rows)
    }
    pub fn validate_expected(&self, expected: RegistrationExpectationV1<'_>) -> Result<(), String> {
        self.rows
            .validate_expected(RegistrationModeV1::Own, expected)
    }
}

impl RegistrationFeatureRowsV1 {
    pub fn validate(&self) -> Result<(), String> {
        let contract = contract_identity_v1()?;
        if self.schema != SCHEMA
            || self.registry_sha256 != contract.registry_sha256
            || self.contract_sha256 != contract.contract_sha256
            || self.roster_sha256 != contract.roster_sha256
            || self.external_episode_id.trim().is_empty()
            || self.counts.len() != WIDTH
            || self.features.len() != WIDTH
            || self.mainboard_size == 0
        {
            return Err("own registration identity or shape differs".into());
        }
        let size = self
            .counts
            .iter()
            .try_fold(0u32, |sum, count| sum.checked_add(*count))
            .ok_or("own mainboard count overflow")?;
        if size != self.mainboard_size {
            return Err("own registration count total differs".into());
        }
        // Bound reconstruction before allocation; the constructor admits exact
        // validated 60-card configurations and stored rows obey that boundary.
        if size as usize != crate::sideboard::REGISTERED_MAINBOARD_SIZE_V1 {
            return Err("own registration selected configuration size differs".into());
        }
        let mut cards = Vec::with_capacity(size as usize);
        for (card, &count) in self.counts.iter().enumerate() {
            cards.extend(std::iter::repeat(card as u16).take(count as usize));
        }
        if mainboard_slice_sha256_v1(&cards) != self.selected_mainboard_sha256 {
            return Err("own registration mainboard digest differs".into());
        }
        let expected = match self.mode {
            RegistrationModeV1::Own => normalized_v1(&self.counts, size),
            RegistrationModeV1::Mean => fixed_mean_features_v1()?,
        };
        if self
            .features
            .iter()
            .zip(&expected)
            .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            return Err("own registration feature values differ from contract".into());
        }
        Ok(())
    }
    pub fn validate_expected(
        &self,
        mode: RegistrationModeV1,
        expected: RegistrationExpectationV1<'_>,
    ) -> Result<(), String> {
        self.validate()?;
        if self.mode != mode
            || self.external_episode_id != expected.external_episode_id
            || self.actor != expected.actor
            || self.selected_mainboard_sha256 != expected.selected_mainboard_sha256
        {
            return Err(
                "own registration episode, actor, mode or selected mainboard binding differs"
                    .into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(name: &str) -> DeckConfigurationV1 {
        let cards = crate::runtime_decks::runtime_deck_by_id(name)
            .unwrap()
            .card_ids
            .to_vec();
        DeckConfigurationV1::new_exact_v1(cards, vec![0; 15]).unwrap()
    }
    #[test]
    fn rows_bind_external_id_physical_seat_and_multiset() {
        let selected = config("Wildfire");
        let context = KnownOwnMainboardV1::new("external-1", PlayerSeatV1::P0, &selected).unwrap();
        let expected = RegistrationExpectationV1 {
            external_episode_id: "external-1",
            actor: PlayerSeatV1::P0,
            selected_mainboard_sha256: selected.mainboard_sha256_v1(),
        };
        let rows = context.rows(RegistrationModeV1::Own).unwrap();
        rows.validate_expected(RegistrationModeV1::Own, expected)
            .unwrap();
        assert!(rows
            .validate_expected(
                RegistrationModeV1::Own,
                RegistrationExpectationV1 {
                    external_episode_id: "external-2",
                    ..expected
                }
            )
            .is_err());
        assert!(rows
            .validate_expected(
                RegistrationModeV1::Own,
                RegistrationExpectationV1 {
                    actor: PlayerSeatV1::P1,
                    ..expected
                }
            )
            .is_err());
        assert!(rows
            .validate_expected(
                RegistrationModeV1::Own,
                RegistrationExpectationV1 {
                    selected_mainboard_sha256: config("Burn").mainboard_sha256_v1(),
                    ..expected
                }
            )
            .is_err());
        let mut shuffled = selected.mainboard().to_vec();
        shuffled.reverse();
        let shuffled = DeckConfigurationV1::new_exact_v1(shuffled, vec![1; 15]).unwrap();
        assert_eq!(
            rows,
            KnownOwnMainboardV1::new("external-1", PlayerSeatV1::P0, &shuffled)
                .unwrap()
                .rows(RegistrationModeV1::Own)
                .unwrap()
        );
        let mut tampered = rows.clone();
        tampered.features[0] = f32::NAN;
        assert!(tampered.validate().is_err());
        let mut tampered = rows.clone();
        tampered.counts[0] += 1;
        assert!(tampered.validate().is_err());
        let mut encoded = serde_json::to_value(rows).unwrap();
        encoded["opponent"] = serde_json::json!("Burn");
        assert!(serde_json::from_value::<RegistrationFeatureRowsV1>(encoded).is_err());
    }
    #[test]
    fn mean_is_fixed_for_all_contexts_and_rejects_tampering() {
        let a = KnownOwnMainboardV1::new("a", PlayerSeatV1::P0, &config("Wildfire"))
            .unwrap()
            .rows(RegistrationModeV1::Mean)
            .unwrap();
        let b = KnownOwnMainboardV1::new("b", PlayerSeatV1::P1, &config("Burn"))
            .unwrap()
            .rows(RegistrationModeV1::Mean)
            .unwrap();
        assert_eq!(a.features, b.features);
        assert_eq!(a.features, fixed_mean_features_v1().unwrap());
        let mut tampered = a;
        tampered.features[0] += 0.01;
        assert!(tampered.validate().is_err());
        assert_eq!(contract_identity_v1().unwrap().roster_sha256, ROSTER_SHA256);
    }
}
