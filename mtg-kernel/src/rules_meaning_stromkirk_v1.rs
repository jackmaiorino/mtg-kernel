//! Exact controller-end-step life-loss root, independent of frozen vector admission.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/stromkirk-bloodthief/v1";
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
    pub controller_end_step_only: bool,
    pub actual_opponent_loss_before_trigger: bool,
    pub intervening_condition_rechecked_at_resolution: bool,
    pub positive_committed_damage_and_loss_payment_count: bool,
    pub gain_zero_and_fully_prevented_damage_do_not_count: bool,
    pub original_endstep_controller_from_committed_observer: bool,
    pub mandatory_controlled_vampire_permanent_target: bool,
    pub noncreature_vampire_permanent_is_legal: bool,
    pub current_control_subtype_protection_and_incarnation_rechecked: bool,
    pub source_departure_or_suppression_after_trigger_does_not_cancel: bool,
    pub exact_target_plus_one_counter: bool,
    pub actual_untap_starts_new_loss_epoch: bool,
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
        include_str!("rules_meaning_stromkirk_v1.rs"),
        include_str!("stromkirk_bloodthief_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("engine.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("life_loss_turn_v1.rs"),
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
    if d.name != "Stromkirk Bloodthief" || !d.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let ts = crate::trigger::triggers_for(card_id);
    if trigger_index != 0
        || d.cost
            != (Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::B)],
                x_count: 0,
            })
        || d.mana_value != 3
        || d.colors != [ManaColor::B]
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Vampire, Subtype::Rogue]
        || (d.power, d.toughness) != (Some(2), Some(2))
        || d.keywords != Keywords::NONE
        || !d.activated_abilities.is_empty()
        || ts.len() != 1
        || ts[0].condition
            != crate::trigger::TriggerCondition::BeginningControllerEndStepIfOpponentLostLife
        || ts[0].home_zone != Zone::Battlefield
        || ts[0].face_index != 0
        || ts[0].intervening_if_kicked
        || ts[0].intervening_if_controls_another_source_card
        || (ts[0].effect)() != crate::stromkirk_bloodthief_v1::root()
        || crate::trigger::trigger_target_spec(card_id)
            != TargetSpec::ControlledPermanentWithAnySubtype([Subtype::Vampire; 4])
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(ErrorV1::WrongRoot);
    }
    let rule = RuleV1 {
        controller_end_step_only: true,
        actual_opponent_loss_before_trigger: true,
        intervening_condition_rechecked_at_resolution: true,
        positive_committed_damage_and_loss_payment_count: true,
        gain_zero_and_fully_prevented_damage_do_not_count: true,
        original_endstep_controller_from_committed_observer: true,
        mandatory_controlled_vampire_permanent_target: true,
        noncreature_vampire_permanent_is_legal: true,
        current_control_subtype_protection_and_incarnation_rechecked: true,
        source_departure_or_suppression_after_trigger_does_not_cancel: true,
        exact_target_plus_one_counter: true,
        actual_untap_starts_new_loss_epoch: true,
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
        crate::stromkirk_bloodthief_v1::root(),
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
