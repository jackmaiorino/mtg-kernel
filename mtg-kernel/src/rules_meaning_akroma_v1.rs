//! Exact Akroma definition roots; no frozen vector or model admission.
use crate::card_def::{CardType, Keywords, Supertype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/akromas-memorial/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    PermanentSpell,
    ControlledCreatureGrant,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub printed_cost: String,
    pub keywords: [String; 5],
    pub protection_colors: [String; 2],
    pub current_source_controller: bool,
    pub current_effective_creature: bool,
    pub active_printed_source_required: bool,
    pub recipient_ability_loss_timestamp_applies: bool,
    pub protection_damage_equipment_blocking_targeting: bool,
    pub damage_cannot_be_prevented_override: bool,
    pub original_color_snapshot_before_simultaneous_departures: bool,
    pub original_visibility_snapshot_private: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub native_model_sampler_admitted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    WrongRoot,
    ForgedMeaning,
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
        include_str!("rules_meaning_akroma_v1.rs"),
        include_str!("controlled_creature_grants_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("trigger.rs"),
        include_str!("state.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
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
pub fn meaning(selector: &SelectorV1, card_id: u16, root: RootV1) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let d = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::WrongRoot)?;
    let permanent = EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    };
    if d.name != "Akroma's Memorial"
        || !d.is_executable()
        || d.types != [CardType::Artifact]
        || d.supertypes != [Supertype::Legendary]
        || !d.colors.is_empty()
        || !d.subtypes.is_empty()
        || d.is_land
        || d.is_token
        || d.ward_cost.is_some()
        || d.equipment.is_some()
        || d.starting_loyalty.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.as_enters_color_choice_v1.is_some()
        || d.chosen_color_creature_boost_v1.is_some()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || !d.produces_mana.is_empty()
        || d.generic_cost_reduction.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.optional_additional_cost.is_some()
        || d.bestow.is_some()
        || d.saga.is_some()
        || d.transform_face.is_some()
        || d.animation.is_some()
        || d.cost.generic != 7
        || !d.cost.pips.is_empty()
        || d.cost.x_count != 0
        || d.mana_value != 7
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || d.controlled_creature_grant != Some(crate::controlled_creature_grants_v1::MEMORIAL)
        || (d.spell_effect)() != Some(permanent.clone())
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.adventure.is_some()
        || d.omen.is_some()
        || d.mana_ability_def.is_some()
        || !d.mana_ability_choices.is_empty()
        || !d.additional_mana_abilities.is_empty()
    {
        return Err(ErrorV1::WrongRoot);
    }
    let bytes = serde_json::to_vec(&(
        card_id,
        root,
        d.name,
        d.cost,
        d.types,
        ["Legendary"],
        d.colors,
        d.subtypes,
        d.keywords,
        permanent,
        crate::controlled_creature_grants_v1::MEMORIAL.keywords,
        crate::controlled_creature_grants_v1::MEMORIAL.protection_color_mask,
    ))
    .map_err(|_| ErrorV1::WrongRoot)?;
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        root,
        root_sha256: digest(&bytes),
        rule: RuleV1 {
            printed_cost: "{7}".into(),
            keywords: ["Flying", "First strike", "Vigilance", "Trample", "Haste"].map(String::from),
            protection_colors: ["Black", "Red"].map(String::from),
            current_source_controller: true,
            current_effective_creature: true,
            active_printed_source_required: true,
            recipient_ability_loss_timestamp_applies: true,
            protection_damage_equipment_blocking_targeting: true,
            damage_cannot_be_prevented_override: true,
            original_color_snapshot_before_simultaneous_departures: true,
            original_visibility_snapshot_private: true,
        },
        native_model_sampler_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if meaning(&record.selector, record.card_id, record.root)? != *record {
        return Err(ErrorV1::ForgedMeaning);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_predecessor_definition_and_foreign_selector_refuse() {
        let id = crate::card_def::card_id_by_name("Forest").unwrap();
        if let Ok(selector) = live_selector() {
            assert_eq!(
                meaning(&selector, id, RootV1::ControlledCreatureGrant),
                Err(ErrorV1::WrongRoot)
            );
            let mut wrong = selector;
            wrong.card_db_hash ^= 1;
            assert_eq!(
                meaning(&wrong, id, RootV1::PermanentSpell),
                Err(ErrorV1::WrongSelector)
            );
        } else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
        }
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_two_roots_and_all_meaning_fields_reconstruct() {
        let id = crate::card_def::card_id_by_name("Akroma's Memorial").unwrap();
        let selector = live_selector().unwrap();
        for root in [RootV1::PermanentSpell, RootV1::ControlledCreatureGrant] {
            let record = meaning(&selector, id, root).unwrap();
            validate(&record).unwrap();
            let mut wrong = record.clone();
            wrong.rule.protection_colors.reverse();
            assert_eq!(validate(&wrong), Err(ErrorV1::ForgedMeaning));
            let mut wrong = record.clone();
            wrong.rule.keywords[4] = "Trample".into();
            assert_eq!(validate(&wrong), Err(ErrorV1::ForgedMeaning));
            let mut wrong = record;
            wrong.rule.original_visibility_snapshot_private = false;
            assert_eq!(validate(&wrong), Err(ErrorV1::ForgedMeaning));
        }
    }
}
