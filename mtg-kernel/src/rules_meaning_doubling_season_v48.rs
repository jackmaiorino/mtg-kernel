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
    /// Direct unmodified loyalty cost, before another replacement applies.
    pub loyalty_activation_cost_excluded: bool,
    pub other_replacement_can_make_cost_or_rule_counter_event_eligible: bool,
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
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("rules_meaning_doubling_season_v48.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("card_def.rs"),
            include_str!("event.rs"),
            include_str!("trigger.rs"),
            include_str!("engine.rs"),
            include_str!("state.rs"),
            include_str!("fdn_raid_v1.rs"),
            include_str!("garruks_uprising_v1.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("standard_statics_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("effect.rs"),
            include_str!("effect/curator_v1.rs"),
            include_str!("effect/loot_v1.rs"),
            include_str!("effect/kykar_v1.rs"),
            include_str!("effect/greatwurm_v1.rs"),
            include_str!("effect/source_token_copy_v1.rs"),
            include_str!("engine/greatwurm_cast_v1.rs"),
            include_str!("effect/sphinx_lore_v1.rs"),
            include_str!("engine/sphinx_lore_cast_v1.rs"),
            include_str!("spell_costs_v1.rs"),
            include_str!("effect/squad_rallier_resolution_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("../build.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("progenitus_v1.rs"),
        ] {
            h.update((source.len() as u64).to_le_bytes());
            h.update(source.as_bytes());
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
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
        other_replacement_can_make_cost_or_rule_counter_event_eligible: counter,
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
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
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
            forged.other_replacement_can_make_cost_or_rule_counter_event_eligible =
                !forged.other_replacement_can_make_cost_or_rule_counter_event_eligible;
            assert!(validate(&forged).is_err());
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

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn genuine_compiled_definition_rejects_foreign_rule_descriptors() {
        use crate::card_def::{CardDef, Keywords, ModeDef, OptionalAdditionalCostDef};
        let id = crate::card_def::card_id_by_name("Doubling Season").unwrap();
        let original = &crate::card_def::CARD_DEFS[usize::from(id)];
        assert!(crate::doubling_season_v48::owns_definition(id));
        // Negative metadata inputs only. None of these forged definitions is
        // inserted in CARD_DEFS or executed as a fabricated printed card.
        let copy = || CardDef {
            flashback: None,
            mode2: None,
            mode3: None,
            escape: None,
            omen: None,
            bestow: None,
            adventure: None,
            transform_face: None,
            saga: None,
            ..*original
        };
        let mut forged = copy();
        forged.optional_additional_cost = Some(OptionalAdditionalCostDef::Bargain);
        assert!(!crate::doubling_season_v48::exact_definition(&forged));
        let donor = &crate::card_def::CARD_DEFS
            [usize::from(crate::card_def::card_id_by_name("Pyroblast").unwrap())];
        let mode = donor.mode2.as_ref().unwrap();
        let mut forged = copy();
        forged.mode2 = Some(ModeDef {
            target_spec: mode.target_spec,
            effect: mode.effect,
        });
        assert!(!crate::doubling_season_v48::exact_definition(&forged));
        let mut forged = copy();
        forged.keywords = Keywords::HASTE;
        assert!(!crate::doubling_season_v48::exact_definition(&forged));
        let mut forged = copy();
        forged.object_name = "different public rules name";
        assert!(!crate::doubling_season_v48::exact_definition(&forged));
        let mut forged = copy();
        forged.damage_replacement_v1 =
            Some(crate::card_def::DamageReplacementDefV1::DoubleControlledSourceToOpponent);
        assert!(!crate::doubling_season_v48::exact_definition(&forged));
        let mut forged = copy();
        forged.changeling = true;
        assert!(!crate::doubling_season_v48::exact_definition(&forged));
        for root in [
            RootV48::EnchantmentSpell,
            RootV48::EffectTokenCreation,
            RootV48::EffectCounterPlacement,
        ] {
            validate(&printed_meaning(id, root).unwrap()).unwrap();
        }
    }
}
