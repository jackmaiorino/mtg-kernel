//! Selected actual attack root; no frozen-vector or model admission.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/ashroot-animist/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub attacks_not_enters_attacking: bool,
    pub original_declaration_controller: bool,
    pub mandatory_another_controlled_creature_target_at_placement: bool,
    pub current_target_controller_creature_and_protection_rechecked: bool,
    pub original_source_owner_battlefield_generation: bool,
    pub source_power_sampled_once_at_resolution: bool,
    pub actual_departure_power_used_after_source_leaves: bool,
    pub returned_source_power_not_used: bool,
    pub simultaneous_departure_power_sampled_before_any_member_leaves: bool,
    pub negative_power_floored_at_zero: bool,
    pub zero_still_grants_trample: bool,
    pub exact_recipient_generation_power_and_toughness_boost: bool,
    pub source_departure_control_or_ability_loss_does_not_cancel: bool,
    pub later_recipient_control_or_counter_changes_keep_resolved_effect: bool,
    pub ability_removal_timestamp_applies_to_granted_trample: bool,
    pub boosts_and_trample_expire_at_actual_cleanup: bool,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    IdentityMismatch,
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for text in [
        SCHEMA,
        include_str!("rules_meaning_ashroot_v1.rs"),
        include_str!("ashroot_animist_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("engine.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("creature_stats_lki_v1.rs"),
        include_str!("effect/temporary_control_v1.rs"),
        include_str!("../build.rs"),
    ] {
        h.update([0]);
        h.update(text.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
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
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(
    selector: &SelectorV1,
    card_id: u16,
    trigger_index: u16,
) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let d = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if d.name != "Ashroot Animist" || !d.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let ts = crate::trigger::triggers_for(card_id);
    if trigger_index != 0
        || d.cost
            != (Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::G)],
                x_count: 0,
            })
        || d.mana_value != 4
        || d.colors != [ManaColor::R, ManaColor::G]
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Lizard, Subtype::Druid]
        || (d.power, d.toughness) != (Some(4), Some(4))
        || d.keywords != Keywords::TRAMPLE
        || !d.activated_abilities.is_empty()
        || ts.len() != 1
        || ts[0].condition != crate::trigger::TriggerCondition::Attacks
        || ts[0].home_zone != Zone::Battlefield
        || ts[0].face_index != 0
        || ts[0].intervening_if_kicked
        || ts[0].intervening_if_controls_another_source_card
        || (ts[0].effect)() != crate::ashroot_animist_v1::root()
        || crate::trigger::trigger_target_spec(card_id) != TargetSpec::AnotherControlledCreature
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(ErrorV1::WrongRoot);
    }
    let rule = RuleV1 {
        attacks_not_enters_attacking: true,
        original_declaration_controller: true,
        mandatory_another_controlled_creature_target_at_placement: true,
        current_target_controller_creature_and_protection_rechecked: true,
        original_source_owner_battlefield_generation: true,
        source_power_sampled_once_at_resolution: true,
        actual_departure_power_used_after_source_leaves: true,
        returned_source_power_not_used: true,
        simultaneous_departure_power_sampled_before_any_member_leaves: true,
        negative_power_floored_at_zero: true,
        zero_still_grants_trample: true,
        exact_recipient_generation_power_and_toughness_boost: true,
        source_departure_control_or_ability_loss_does_not_cancel: true,
        later_recipient_control_or_counter_changes_keep_resolved_effect: true,
        ability_removal_timestamp_applies_to_granted_trample: true,
        boosts_and_trample_expire_at_actual_cleanup: true,
    };
    let bytes = serde_json::to_vec(&(
        card_id,
        trigger_index,
        d.cost,
        d.colors,
        d.types,
        d.subtypes,
        d.power,
        d.toughness,
        crate::ashroot_animist_v1::root(),
        &rule,
    ))
    .expect("selected root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        trigger_index,
        root_sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        rule,
        public_human_model_and_sampler_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id, record.trigger_index)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrong_selector_and_old_catalog_refuse_before_card_lookup() {
        let Ok(selector) = live_selector() else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        let mut wrong = selector.clone();
        wrong.schema.push('x');
        assert_eq!(meaning(&wrong, u16::MAX, 0), Err(ErrorV1::WrongSelector));
        assert_eq!(
            meaning(
                &selector,
                crate::card_def::card_id_by_name("Lightning Bolt").unwrap(),
                0
            ),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
}
