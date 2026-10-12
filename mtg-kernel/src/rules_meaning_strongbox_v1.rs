//! Exact selected Strongbox recipe, separate from frozen vector/model meanings.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::EffectOp;
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/strongbox/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub catalog_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub trigger_index: u16,
    pub root_sha256: String,
    pub genuine_self_entry: bool,
    pub captured_controller: bool,
    pub raid_at_capture_and_resolution: bool,
    pub declaration_must_be_nonempty_successful_current_turn: bool,
    pub targets: u8,
    pub library_controller: bool,
    pub exile_requested: u8,
    pub choose_exactly_one_surviving_exile: bool,
    pub empty_no_choice_no_permission: bool,
    pub one_survivor_automatic: bool,
    pub ordinary_timing_costs_land_quota: bool,
    pub permission_holder_original_controller: bool,
    pub exact_new_exile_generation: bool,
    pub permission_ends_at_holder_next_turn_cleanup: bool,
    pub source_survival_required: bool,
    pub stage_and_completion_owner_required: bool,
    pub registration_or_model_admission: bool,
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_strongbox_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("effect/exile_top_choose_v1.rs"),
        include_str!("effect/strongbox_permission_v1.rs"),
        include_str!("effect/squad_rallier_resolution_v1.rs"),
        include_str!("raid_entry_v1.rs"),
        include_str!("raid_entry_custody_v1.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("extra_turn_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
    ] {
        h.update([0]);
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector() -> Result<SelectorV1, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Strongbox meaning requires selected Limited catalog".into());
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        catalog_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    })
}
pub fn meaning(
    selector: &SelectorV1,
    card_id: u16,
    trigger_index: u16,
) -> Result<MeaningV1, String> {
    if *selector != live_selector()? {
        return Err("wrong Strongbox source/catalog selector".into());
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or("unknown Strongbox definition")?;
    let abilities = crate::trigger::triggers_for(card_id);
    let root = crate::trigger::strongbox_raider_effect();
    if def.name != "Strongbox Raider"
        || !def.is_executable()
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Orc, Subtype::Pirate]
        || def.cost.generic != 2
        || def.cost.pips != [Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)]
        || def.colors != [ManaColor::R]
        || def.cost.x_count != 0
        || def.target_spec != TargetSpec::None
        || (def.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: crate::effect::ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || crate::trigger::target_spec_for_trigger(card_id, &root) != Some(TargetSpec::None)
        || def.mana_value != 4
        || def.power != Some(5)
        || def.toughness != Some(2)
        || def.keywords != Keywords::NONE
        || !def.activated_abilities.is_empty()
        || trigger_index != 0
        || abilities.len() != 1
        || !crate::raid_entry_v1::owns_definition(card_id)
        || abilities[0].home_zone != Zone::Battlefield
        || abilities[0].face_index != 0
        || (abilities[0].effect)() != root
        || !matches!(root, EffectOp::Conditional { .. })
    {
        return Err("Strongbox definition/root/metadata differs from printed recipe".into());
    }
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        trigger_index,
        root_sha256: hash(
            &serde_json::to_vec(&(
                card_id,
                trigger_index,
                def.cost,
                def.types,
                def.subtypes,
                def.colors,
                def.power,
                def.toughness,
                def.keywords,
                abilities[0].condition,
                abilities[0].home_zone,
                abilities[0].face_index,
                &root,
            ))
            .map_err(|e| e.to_string())?,
        ),
        genuine_self_entry: true,
        captured_controller: true,
        raid_at_capture_and_resolution: true,
        declaration_must_be_nonempty_successful_current_turn: true,
        targets: 0,
        library_controller: true,
        exile_requested: 2,
        choose_exactly_one_surviving_exile: true,
        empty_no_choice_no_permission: true,
        one_survivor_automatic: true,
        ordinary_timing_costs_land_quota: true,
        permission_holder_original_controller: true,
        exact_new_exile_generation: true,
        permission_ends_at_holder_next_turn_cleanup: true,
        source_survival_required: false,
        stage_and_completion_owner_required: true,
        registration_or_model_admission: false,
    })
}
pub fn validate(selector: &SelectorV1, artifact: &MeaningV1) -> Result<(), String> {
    if *artifact != meaning(selector, artifact.card_id, artifact.trigger_index)? {
        return Err("forged Strongbox meaning fact".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_foreign_definition_and_wrong_source_selector_refuse() {
        let id = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        if let Ok(selector) = live_selector() {
            assert!(meaning(&selector, id, 0).is_err());
            let mut wrong = selector.clone();
            wrong.producer_sha256.clear();
            assert!(meaning(&wrong, id, 0).is_err());
            let mut wrong = selector;
            wrong.catalog_hash ^= 1;
            assert!(meaning(&wrong, id, 0).is_err());
        } else {
            assert!(!cfg!(all(
                feature = "limited-fdn-fixtures",
                not(feature = "standard-magezero-fixtures")
            )));
        }
    }
}
