//! Exact selected static roots. Frozen vectors cannot encode these replacements.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA: &str = "mtg-kernel/rules-meaning/doubling-season/v48";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV48 {
    EnchantmentSpell,
    EffectTokenCreation,
    EffectCounterPlacement,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV48 {
    pub schema: String,
    pub catalog_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV48 {
    pub selector: SelectorV48,
    pub card_id: u16,
    pub root: RootV48,
    pub root_sha256: String,
    pub printed_cost: String,
    pub roots_in_printed_order: Vec<RootV48>,
    pub while_source_has_printed_abilities_on_battlefield: bool,
    pub current_source_controller: bool,
    pub positive_effect_token_creation_only: bool,
    pub all_created_token_characteristics_preserved: bool,
    pub permanent_spell_copy_becoming_token_is_not_creation: bool,
    pub positive_effect_counter_placement_only: bool,
    pub permanent_recipient_current_controller: bool,
    pub counter_placing_actor_may_be_opponent: bool,
    pub counters_of_each_kind_doubled: bool,
    pub entry_counters_included: bool,
    pub loyalty_activation_cost_excluded: bool,
    pub independent_sources_each_apply_once: bool,
    pub original_count_multiplied_without_arbitrary_cap: bool,
    pub safe_consumer_admitted: bool,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_doubling_season_v48.rs"),
        include_str!("doubling_season_v48.rs"),
        include_str!("card_def.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("planeswalker_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("../build.rs"),
    ] {
        h.update((source.len() as u64).to_le_bytes());
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn printed_meaning(card_id: u16, root: RootV48) -> Result<MeaningV48, String> {
    if !crate::doubling_season_v48::owns_definition(card_id) {
        return Err("Doubling Season exact printing absent or changed".into());
    }
    let d = &crate::card_def::CARD_DEFS[usize::from(card_id)];
    let token = root == RootV48::EffectTokenCreation;
    let counter = root == RootV48::EffectCounterPlacement;
    let root_sha256 = digest(
        &serde_json::to_vec(&(
            SCHEMA,
            card_id,
            root,
            d.cost.generic,
            d.cost.x_count,
            d.cost.pips,
            d.mana_value,
            d.types,
            d.colors,
            d.token_counter_replacement_v48,
            (d.spell_effect)(),
        ))
        .map_err(|e| e.to_string())?,
    );
    Ok(MeaningV48 {
        selector: SelectorV48 {
            schema: SCHEMA.into(),
            catalog_hash: crate::card_def::KERNEL_CARDDB_HASH,
            producer_sha256: producer_sha256(),
        },
        card_id,
        root,
        root_sha256,
        printed_cost: "{4}{G}".into(),
        roots_in_printed_order: vec![
            RootV48::EffectTokenCreation,
            RootV48::EffectCounterPlacement,
        ],
        while_source_has_printed_abilities_on_battlefield: token || counter,
        current_source_controller: token || counter,
        positive_effect_token_creation_only: token,
        all_created_token_characteristics_preserved: token,
        permanent_spell_copy_becoming_token_is_not_creation: token,
        positive_effect_counter_placement_only: counter,
        permanent_recipient_current_controller: counter,
        counter_placing_actor_may_be_opponent: counter,
        counters_of_each_kind_doubled: counter,
        entry_counters_included: counter,
        loyalty_activation_cost_excluded: counter,
        independent_sources_each_apply_once: token || counter,
        original_count_multiplied_without_arbitrary_cap: token || counter,
        safe_consumer_admitted: false,
    })
}
pub fn validate(value: &MeaningV48) -> Result<(), String> {
    if *value == printed_meaning(value.card_id, value.root)? {
        Ok(())
    } else {
        Err("Doubling Season rooted meaning differs".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_registered_other_printing_is_not_a_replacement_root() {
        let id = crate::card_def::card_id_by_name("Forest").unwrap();
        for root in [
            RootV48::EnchantmentSpell,
            RootV48::EffectTokenCreation,
            RootV48::EffectCounterPlacement,
        ] {
            assert!(printed_meaning(id, root).is_err());
        }
    }
    #[test]
    #[ignore = "future actual Doubling Season registration required"]
    fn actual_printed_roots_and_complete_forged_facts() {
        let id = crate::card_def::card_id_by_name("Doubling Season").unwrap();
        for root in [
            RootV48::EnchantmentSpell,
            RootV48::EffectTokenCreation,
            RootV48::EffectCounterPlacement,
        ] {
            let original = printed_meaning(id, root).unwrap();
            validate(&original).unwrap();
            let mut forged = original.clone();
            forged.loyalty_activation_cost_excluded = !forged.loyalty_activation_cost_excluded;
            assert!(validate(&forged).is_err());
            let mut forged = original.clone();
            forged.permanent_recipient_current_controller =
                !forged.permanent_recipient_current_controller;
            assert!(validate(&forged).is_err());
            let mut forged = original.clone();
            forged.root_sha256.clear();
            assert!(validate(&forged).is_err());
            let mut forged = original;
            forged.selector.catalog_hash ^= 1;
            assert!(validate(&forged).is_err());
        }
    }
}
