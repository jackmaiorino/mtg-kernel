//! Explicit public auxiliary features for a future model successor.
//! This does not replace the frozen V4 encoder or imply a trained model.
use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::mana::{ManaColor, Pip};
use crate::policy_observation_v6::ObservationV6;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub const SCHEMA: &str = "mtg-kernel-structured-public-features/v1";
pub const STATE_WIDTH: usize = 6;
pub const OBJECT_WIDTH: usize = 32;
const REGISTRY: &[u8] = include_bytes!("../../data/cards_v1.json");
const CONTRACT: &[u8] = include_bytes!("../../data/public_cost_features_v1/contract.json");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicFeatureCatalogV1 {
    pub schema: String,
    pub registry_sha256: String,
    pub contract_sha256: String,
    pub card_db_hash: u64,
    pub card_names: Vec<String>,
    pub rows: Vec<Vec<f32>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicFeatureRowsV1 {
    pub schema: String,
    pub registry_sha256: String,
    pub contract_sha256: String,
    pub state: Vec<f32>,
    pub objects: Vec<Vec<f32>>,
}

fn pair_index(a: ManaColor, b: ManaColor) -> Result<usize, String> {
    let (a, b) = (a.pool_index().min(b.pool_index()), a.pool_index().max(b.pool_index()));
    if a == b { return Err("same-color hybrid is outside this contract".into()); }
    let mut index = 17;
    for i in 0..6 { for j in i+1..6 {
        if (i,j) == (a,b) { return Ok(index); }
        index += 1;
    }}
    Err("invalid hybrid color".into())
}

fn build_catalog() -> Result<PublicFeatureCatalogV1, String> {
    let contract: serde_json::Value = serde_json::from_slice(CONTRACT).map_err(|e| e.to_string())?;
    if contract["card_db_hash"].as_str() != Some(format!("{KERNEL_CARDDB_HASH:016x}").as_str()) {
        return Err("structured-feature contract registry differs".into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(REGISTRY).map_err(|e| e.to_string())?;
    let cards = metadata["cards"].as_array().ok_or("missing cards")?;
    if cards.len() != CARD_DEFS.len() { return Err("compiled and public registry lengths differ".into()); }
    let mut rows = Vec::new();
    let mut names = Vec::new();
    for (definition, printed) in CARD_DEFS.iter().zip(cards) {
        if printed["name"].as_str() != Some(definition.name) { return Err("compiled registry order differs".into()); }
        let symbols = printed["mana_cost"].as_str().ok_or("missing printed mana cost")?;
        let cost = &definition.cost;
        let mut row = vec![0.0; OBJECT_WIDTH];
        row[0] = 1.0;
        row[1] = f32::from(!symbols.is_empty());
        row[2] = (usize::from(cost.generic) + cost.pips.len()) as f32 / 16.0;
        row[3] = f32::from(cost.generic) / 16.0;
        row[4] = f32::from(cost.x_count) / 4.0;
        for pip in cost.pips {
            let index = match *pip {
                Pip::Colored(color) => 5 + color.pool_index(),
                Pip::Phyrexian(color) => 11 + color.pool_index(),
                Pip::Hybrid(a,b) => pair_index(a,b)?,
            };
            row[index] += 0.125;
        }
        rows.push(row);
        names.push(definition.name.to_owned());
    }
    Ok(PublicFeatureCatalogV1 {
        schema: SCHEMA.into(), registry_sha256: format!("{:x}", Sha256::digest(REGISTRY)),
        contract_sha256: format!("{:x}", Sha256::digest(CONTRACT)), card_db_hash: KERNEL_CARDDB_HASH,
        card_names: names, rows,
    })
}

pub fn catalog_v1() -> Result<&'static PublicFeatureCatalogV1, String> {
    static CATALOG: OnceLock<Result<PublicFeatureCatalogV1, String>> = OnceLock::new();
    CATALOG.get_or_init(build_catalog).as_ref().map_err(Clone::clone)
}

/// Caller supplies only the object tokens from the same validated actor V4
/// decision. This pure auxiliary transform cannot access a live game state.
pub(crate) fn from_actor_v4_v1(observation: &ObservationV6, tokens: &[i64]) -> Result<PublicFeatureRowsV1, String> {
    if observation.card_db_hash != KERNEL_CARDDB_HASH { return Err("observation card registry differs".into()); }
    let catalog = catalog_v1()?;
    let mut state = vec![0.0; STATE_WIDTH];
    for effect in &observation.projection.surface.continuous_effects {
        let mask = effect.prevent_damage_from_color_mask;
        if mask > 31 { return Err("invalid public prevention color mask".into()); }
        if (mask != 0 || effect.damage_cannot_be_prevented) && !effect.global {
            return Err("scoped prevention is outside the global feature contract".into());
        }
        for (bit, value) in state.iter_mut().enumerate().take(5) {
            if mask & (1 << bit) != 0 { *value = 1.0; }
        }
        if effect.damage_cannot_be_prevented { state[5] = 1.0; }
    }
    let objects = tokens.iter().map(|token| {
        if *token == 0 { return Ok(vec![0.0; OBJECT_WIDTH]); }
        let index = usize::try_from(token.checked_sub(1).ok_or("invalid visible card token")?)
            .map_err(|_| "invalid visible card token")?;
        catalog.rows.get(index).cloned().ok_or_else(|| "visible card token outside compiled registry".into())
    }).collect::<Result<Vec<_>, String>>()?;
    Ok(PublicFeatureRowsV1 { schema: SCHEMA.into(), registry_sha256: catalog.registry_sha256.clone(),
        contract_sha256: catalog.contract_sha256.clone(), state, objects })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compiled_front_cost_rows_include_hybrid_phyrexian_x_and_no_cost() {
        let catalog = catalog_v1().unwrap();
        let row = |name: &str| &catalog.rows[catalog.card_names.iter().position(|n| n == name).unwrap()];
        assert_eq!(row("Counterspell")[6], 0.25);
        assert_eq!(row("Nyxborn Hydra")[4], 0.25);
        assert_eq!(row("Gut Shot")[14], 0.125);
        assert_eq!(row("Burning-Tree Emissary")[pair_index(ManaColor::R, ManaColor::G).unwrap()], 0.25);
        assert_eq!(row("Island")[1], 0.0);
        assert_eq!(row("Lotus Petal")[1], 1.0);
    }
}
