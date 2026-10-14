//! Precise definition-owned printed rule; no vector, tensor or model admission.
use crate::card_def::{CardType, Subtype, TargetSpec, CARD_DEFS};
use crate::mana::{ManaColor, Pip};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/vampire-gourmand/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    ForgedMeaning,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub printed_cost: String,
    pub timing: String,
    pub target_spec: TargetSpec,
    pub optional_resolution_cost: bool,
    pub sacrifice_minimum: u8,
    pub sacrifice_maximum: u8,
    pub sacrificed_permanent_must_be_creature: bool,
    pub controlled_by_original_attack_controller: bool,
    pub another_excludes_original_incarnation: bool,
    pub returned_new_source_incarnation_may_be_sacrificed: bool,
    pub sacrifice_is_not_targeting: bool,
    pub protection_does_not_prevent_sacrifice_selection: bool,
    pub actual_replacement_exile_pays_sacrifice: bool,
    pub prevented_battlefield_departure_does_not_pay: bool,
    pub draw_count_after_payment: u8,
    pub draw_survives_original_source_departure: bool,
    pub unblockable_binds_original_source_incarnation: bool,
    pub rule_survives_later_ability_suppression: bool,
    pub rule_follows_later_control_change: bool,
    pub duration: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub trigger_index: u16,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub public_human_model_and_sampler_admitted: bool,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_gourmand_v1.rs"),
        include_str!("vampire_gourmand_v1.rs"),
        include_str!("vampire_gourmand_dispatch_v2.rs"),
        include_str!("effect/vampire_gourmand_resolution_v1.rs"),
        include_str!("effect/standard_selection_v1.rs"),
        include_str!("effect.rs"),
        include_str!("trigger.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector() -> Result<SelectorV1, ErrorV1> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV1::UnsupportedCatalog);
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    })
}
pub fn meaning(selector: &SelectorV1, id: u16, index: u16) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(id as usize)
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Vampire Gourmand" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let abilities = crate::trigger::triggers_for(id);
    let trigger = abilities.get(index as usize).ok_or(ErrorV1::WrongRoot)?;
    let root = (trigger.effect)();
    if index != 0
        || abilities.len() != 1
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Vampire]
        || def.colors != [ManaColor::B]
        || (def.power, def.toughness, def.mana_value) != (Some(2), Some(2), 2)
        || def.cost.generic != 1
        || def.cost.pips != [Pip::Colored(ManaColor::B)]
        || def.cost.x_count != 0
        || def.target_spec != TargetSpec::None
        || !def.activated_abilities.is_empty()
        || trigger.condition != crate::trigger::TriggerCondition::Attacks
        || trigger.home_zone != crate::state::Zone::Battlefield
        || trigger.face_index != 0
        || trigger.intervening_if_kicked
        || trigger.intervening_if_controls_another_source_card
        || root != crate::vampire_gourmand_v1::program()
        || crate::trigger::target_spec_for_trigger(id, &root) != Some(TargetSpec::None)
    {
        return Err(ErrorV1::WrongRoot);
    }
    let bytes = serde_json::to_vec(&(
        id,
        index,
        def.cost,
        def.types,
        def.subtypes,
        def.colors,
        def.power,
        def.toughness,
        "Attacks",
        trigger.home_zone,
        trigger.face_index,
        trigger.intervening_if_kicked,
        trigger.intervening_if_controls_another_source_card,
        root,
    ))
    .map_err(|_| ErrorV1::WrongRoot)?;
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id: id,
        trigger_index: index,
        root_sha256: digest(&bytes),
        rule: RuleV1 {
            printed_cost: "{1}{B}".into(),
            timing: "this_creature_attacks".into(),
            target_spec: TargetSpec::None,
            optional_resolution_cost: true,
            sacrifice_minimum: 0,
            sacrifice_maximum: 1,
            sacrificed_permanent_must_be_creature: true,
            controlled_by_original_attack_controller: true,
            another_excludes_original_incarnation: true,
            returned_new_source_incarnation_may_be_sacrificed: true,
            sacrifice_is_not_targeting: true,
            protection_does_not_prevent_sacrifice_selection: true,
            actual_replacement_exile_pays_sacrifice: true,
            prevented_battlefield_departure_does_not_pay: true,
            draw_count_after_payment: 1,
            draw_survives_original_source_departure: true,
            unblockable_binds_original_source_incarnation: true,
            rule_survives_later_ability_suppression: true,
            rule_follows_later_control_change: true,
            duration: "until_next_cleanup_including_repeated_cleanup".into(),
        },
        public_human_model_and_sampler_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if meaning(&record.selector, record.card_id, record.trigger_index)? != *record {
        return Err(ErrorV1::ForgedMeaning);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_selector_rejects_foreign_and_predecessor_catalogs() {
        if !cfg!(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )) {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        }
        let mut selector = live_selector().unwrap();
        selector.schema.push_str("/wrong");
        assert_eq!(meaning(&selector, 0, 0), Err(ErrorV1::WrongSelector));
        let selector = live_selector().unwrap();
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        assert_eq!(
            meaning(&selector, forest, 0),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
}
