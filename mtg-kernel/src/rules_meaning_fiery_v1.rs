//! Selected definition-owned spell meaning. No vector or model admission.
use crate::card_def::{CardType, TargetSpec, CARD_DEFS};
use crate::effect::EffectOp;
use crate::mana::{ManaColor, Pip};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/fiery-annihilation/v1";

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
    WrongRoot,
    ForgedMeaning,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub printed_cost: String,
    pub target_spec: TargetSpec,
    pub creature_target_count: u8,
    pub equipment_target_minimum: u8,
    pub equipment_target_maximum: u8,
    pub equipment_attached_to_original_creature_incarnation: bool,
    pub each_target_rechecks_its_original_incarnation_and_legality: bool,
    pub all_illegal_targets_counter_spell_without_instructions: bool,
    pub damage_count: u8,
    pub damage_then_equipment_exile_then_death_replacement: bool,
    pub prevention_does_not_cancel_later_instructions: bool,
    pub illegal_creature_does_not_cancel_independently_legal_equipment: bool,
    pub illegal_equipment_does_not_cancel_legal_creature_damage_or_replacement: bool,
    pub replacement_requires_legal_creature_target: bool,
    pub replacement_binds_original_creature_incarnation: bool,
    pub replacement_applies_to_battlefield_to_graveyard_only: bool,
    pub replacement_survives_spell_source_departure: bool,
    pub replacement_installed_before_post_spell_state_based_actions: bool,
    pub duration: String,
    pub expires_at_actual_cleanup_including_repeated_cleanup: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub spell_mode: u8,
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
        include_str!("rules_meaning_fiery_v1.rs"),
        include_str!("fiery_annihilation_v1.rs"),
        include_str!("effect.rs"),
        include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
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
pub fn meaning(selector: &SelectorV1, card_id: u16, spell_mode: u8) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS.get(card_id as usize).ok_or(ErrorV1::WrongRoot)?;
    if spell_mode != 0
        || def.name != "Fiery Annihilation"
        || !def.is_executable()
        || def.types != [CardType::Instant]
        || !def.subtypes.is_empty()
        || !def.supertypes.is_empty()
        || def.colors != [ManaColor::R]
        || (def.power, def.toughness, def.mana_value) != (None, None, 3)
        || def.cost.generic != 2
        || def.cost.pips != [Pip::Colored(ManaColor::R)]
        || def.cost.x_count != 0
        || def.target_spec != TargetSpec::CreatureThenUpToOneAttachedEquipment
        || (def.spell_effect)() != Some(EffectOp::FieryAnnihilation)
        || def.mode2.is_some()
        || def.mode3.is_some()
        || !def.additional_modes.is_empty()
        || def.omen.is_some()
        || def.adventure.is_some()
        || !def.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
    {
        return Err(ErrorV1::WrongRoot);
    }
    let root = serde_json::to_vec(&(
        card_id,
        spell_mode,
        def.cost,
        def.types,
        def.colors,
        def.target_spec,
        (def.spell_effect)(),
    ))
    .map_err(|_| ErrorV1::WrongRoot)?;
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        spell_mode,
        root_sha256: digest(&root),
        rule: RuleV1 {
            printed_cost: "{2}{R}".into(),
            target_spec: TargetSpec::CreatureThenUpToOneAttachedEquipment,
            creature_target_count: 1,
            equipment_target_minimum: 0,
            equipment_target_maximum: 1,
            equipment_attached_to_original_creature_incarnation: true,
            each_target_rechecks_its_original_incarnation_and_legality: true,
            all_illegal_targets_counter_spell_without_instructions: true,
            damage_count: 5,
            damage_then_equipment_exile_then_death_replacement: true,
            prevention_does_not_cancel_later_instructions: true,
            illegal_creature_does_not_cancel_independently_legal_equipment: true,
            illegal_equipment_does_not_cancel_legal_creature_damage_or_replacement: true,
            replacement_requires_legal_creature_target: true,
            replacement_binds_original_creature_incarnation: true,
            replacement_applies_to_battlefield_to_graveyard_only: true,
            replacement_survives_spell_source_departure: true,
            replacement_installed_before_post_spell_state_based_actions: true,
            duration: "this_actual_turn".into(),
            expires_at_actual_cleanup_including_repeated_cleanup: true,
        },
        public_human_model_and_sampler_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record == meaning(&record.selector, record.card_id, record.spell_mode)? {
        Ok(())
    } else {
        Err(ErrorV1::ForgedMeaning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_selected_catalog_foreign_root_and_wrong_selector_refuse() {
        match live_selector() {
            Ok(mut selector) => {
                let id = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
                assert_eq!(meaning(&selector, id, 0), Err(ErrorV1::WrongRoot));
                selector.schema.push('x');
                assert_eq!(meaning(&selector, id, 0), Err(ErrorV1::WrongSelector));
            }
            Err(error) => assert_eq!(error, ErrorV1::UnsupportedCatalog),
        }
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn future_printed_definition_meaning_and_coherent_rewrites_refuse() {
        let selector = live_selector().unwrap();
        let id = crate::card_def::card_id_by_name("Fiery Annihilation")
            .expect("actual Fiery registration");
        let original = meaning(&selector, id, 0).unwrap();
        validate(&original).unwrap();
        assert_eq!(meaning(&selector, id, 1), Err(ErrorV1::WrongRoot));
        for change in 0..5 {
            let mut bad = original.clone();
            match change {
                0 => bad.rule.equipment_target_maximum = 2,
                1 => bad.rule.damage_count = 4,
                2 => bad.rule.replacement_binds_original_creature_incarnation = false,
                3 => bad.rule.duration = "this_round".into(),
                _ => bad.root_sha256.push('x'),
            }
            assert_eq!(validate(&bad), Err(ErrorV1::ForgedMeaning));
        }
    }
}
