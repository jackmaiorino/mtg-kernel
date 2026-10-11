//! Precise selected mana recipe; no frozen vector or model admission.
use crate::{
    card_def::{
        self, CardType, ManaAbilityAmountDef, ManaAbilityCostDef, Subtype, TargetSpec, CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    state::Zone,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub ability_index: u16,
    pub printed_cost: String,
    pub activation_zone: Zone,
    pub tap_source: bool,
    pub tap_symbol_sickness_applies_only_while_creature_without_haste: bool,
    pub immediate_mana_ability_without_stack_or_targets: bool,
    pub color_query_uses_current_control_and_effective_battlefield_colors: bool,
    pub source_included: bool,
    pub colorless_is_not_a_color: bool,
    pub one_unit_of_each_distinct_color_added_simultaneously: bool,
    pub all_output_pool_cells_preflight_before_any_cost_or_use_mutation: bool,
    pub explicit_activation_only: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    WrongRoot,
    IdentityMismatch,
}
pub fn live_selector() -> Result<SelectorV1, ErrorV1> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV1::UnsupportedCatalog);
    }
    Ok(SelectorV1 {
        schema: "mtg-kernel/rules-meaning/bloom-tender/v1".into(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        producer_sha256: crate::bloom_tender_public_v1::producer_sha256_v1(),
    })
}
pub fn meaning(selector: &SelectorV1, card_id: u16) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS.get(card_id as usize).ok_or(ErrorV1::WrongRoot)?;
    if def.name != "Bloom Tender"
        || !def.is_executable()
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Elf, Subtype::Druid]
        || def.cost.generic != 1
        || def.cost.x_count != 0
        || def.cost.pips != [Pip::Colored(ManaColor::G)]
        || def.colors != [ManaColor::G]
        || (def.power, def.toughness) != (Some(1), Some(1))
        || def.target_spec != TargetSpec::None
        || def.mana_ability_choices != [ManaColor::G]
        || !def.activated_abilities.is_empty()
        || !def.additional_mana_abilities.is_empty()
        || !def.restricted_mana_abilities.is_empty()
        || def.mana_ability_includes_chosen_color
        || !matches!(
            (def.spell_effect)(),
            Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield
            })
        )
        || def.mana_ability_def.is_none_or(|a| {
            a.cost != ManaAbilityCostDef::TapSelf
                || a.amount != ManaAbilityAmountDef::EachControlledPermanentColor
                || a.controller_damage != 0
                || a.max_activations_per_turn.is_some()
        })
    {
        return Err(ErrorV1::WrongRoot);
    }
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        ability_index: 0,
        printed_cost: "{1}{G}".into(),
        activation_zone: Zone::Battlefield,
        tap_source: true,
        tap_symbol_sickness_applies_only_while_creature_without_haste: true,
        immediate_mana_ability_without_stack_or_targets: true,
        color_query_uses_current_control_and_effective_battlefield_colors: true,
        source_included: true,
        colorless_is_not_a_color: true,
        one_unit_of_each_distinct_color_added_simultaneously: true,
        all_output_pool_cells_preflight_before_any_cost_or_use_mutation: true,
        explicit_activation_only: true,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record == meaning(&record.selector, record.card_id)? {
        Ok(())
    } else {
        Err(ErrorV1::IdentityMismatch)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_catalog_and_wrong_registered_root_refuse() {
        match live_selector() {
            Ok(selector) => {
                let id = card_def::card_id_by_name("Llanowar Elves").unwrap();
                assert_eq!(meaning(&selector, id), Err(ErrorV1::WrongRoot));
                let mut wrong = selector;
                wrong.schema.push('x');
                assert_eq!(meaning(&wrong, id), Err(ErrorV1::WrongSelector));
            }
            Err(error) => assert_eq!(error, ErrorV1::UnsupportedCatalog),
        }
    }
}
