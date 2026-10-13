//! Printed Hunter semantics; distinct from every frozen vector/model vocabulary.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use crate::trigger::TriggerCondition;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/high-society-hunter/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    CreatureSpell,
    Attack,
    OtherNontokenDeath,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub ordered_facts: Vec<String>,
    pub public_human_model_sampler_admitted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    IdentityMismatch,
}

pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for text in [
        SCHEMA,
        include_str!("rules_meaning_high_society_hunter_v1.rs"),
        include_str!("high_society_hunter_v1.rs"),
        include_str!("creature_death_capture_v1.rs"),
        include_str!("effect/standard_selection_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
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
pub(crate) fn exact_definition(id: u16) -> bool {
    let Some(d) = CARD_DEFS.get(usize::from(id)) else {
        return false;
    };
    let triggers = crate::trigger::triggers_for(id);
    d.name == "High-Society Hunter"
        && d.object_name == d.name
        && d.is_executable()
        && d.cost
            == Cost {
                generic: 3,
                pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)],
                x_count: 0,
            }
        && d.mana_value == 5
        && d.colors == [ManaColor::B]
        && d.types == [CardType::Creature]
        && d.subtypes == [Subtype::Vampire, Subtype::Noble]
        && d.supertypes.is_empty()
        && d.power == Some(5)
        && d.toughness == Some(3)
        && d.keywords == Keywords::FLYING
        && d.target_spec == TargetSpec::None
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        && d.activated_abilities.is_empty()
        && !d.is_token
        && !d.is_land
        && !d.changeling
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.optional_additional_cost.is_none()
        && d.flashback.is_none()
        && d.escape.is_none()
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.omen.is_none()
        && d.saga.is_none()
        && d.bestow.is_none()
        && d.adventure.is_none()
        && d.transform_face.is_none()
        && d.ward_cost.is_none()
        && d.controlled_creature_grant.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.equipment.is_none()
        && d.generic_cost_reduction.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.starting_loyalty.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.produces_mana.is_empty()
        && (d.mana_ability)().is_none()
        && d.kicked_target_spec.is_none()
        && d.mana_ability_choices.is_empty()
        && !d.enters_battlefield_tapped
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.enters_battlefield_tapped_unless.is_none()
        && d.attachment.is_none()
        && !d.delve
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.additional_mana_ability_conditions.is_empty()
        && d.restricted_mana_abilities.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && triggers.len() == 2
        && triggers.iter().enumerate().all(|(i, a)| {
            a.home_zone == Zone::Battlefield
                && a.face_index == 0
                && !a.intervening_if_kicked
                && !a.intervening_if_controls_another_source_card
                && a.condition
                    == if i == 0 {
                        TriggerCondition::Attacks
                    } else {
                        TriggerCondition::OtherNontokenCreatureDies
                    }
                && (a.effect)()
                    == if i == 0 {
                        crate::high_society_hunter_v1::attack_program()
                    } else {
                        crate::high_society_hunter_v1::death_program()
                    }
        })
}
pub fn meaning(selector: &SelectorV1, id: u16, root: RootV1) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    if !exact_definition(id) {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let (program,facts)=match root {
        RootV1::CreatureSpell=>((CARD_DEFS[usize::from(id)].spell_effect)().unwrap(),vec![
            "ordinary_3BB_creature_spell_no_targets_or_timing_bypass",
            "black_5_3_vampire_noble_flying"]),
        RootV1::Attack=>(crate::high_society_hunter_v1::attack_program(),vec![
            "actual_attack_declaration_captures_original_ability_controller_source_owner_and_incarnation",
            "untargeted_original_controller_may_sacrifice_zero_or_one_other_currently_controlled_creature",
            "sacrifice_to_exile_pays_but_prevented_battlefield_departure_does_not",
            "only_successful_payment_puts_one_plus_one_counter_on_exact_original_battlefield_source",
            "source_control_definition_or_ability_changes_do_not_redirect_or_cancel_counter",
            "source_departure_or_reentry_skips_counter_not_payment"]),
        RootV1::OtherNontokenDeath=>(crate::high_society_hunter_v1::death_program(),vec![
            "prebatch_event_time_active_printed_battlefield_source_and_original_controller",
            "each_other_effective_creature_nontoken_battlefield_to_graveyard_any_controller",
            "simultaneous_source_death_still_observes_each_other_nontoken_death",
            "ordinary_apnap_trigger_publication_then_one_actual_draw_by_captured_controller",
            "no_target_no_optional_payment_no_intervening_condition",
            "source_departure_control_or_ability_changes_after_capture_do_not_cancel_draw",
            "empty_library_remains_an_actual_draw_attempt"]),
    };
    let bytes = serde_json::to_vec(&(id, root, program)).expect("typed root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id: id,
        root,
        root_sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        ordered_facts: facts.into_iter().map(str::to_owned).collect(),
        public_human_model_sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV1) -> Result<(), ErrorV1> {
    if *value != meaning(&value.selector, value.card_id, value.root)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_foreign_recipe_and_forged_selector_are_readonly_refused() {
        if let Ok(selector) = live_selector() {
            let id = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
            for root in [
                RootV1::CreatureSpell,
                RootV1::Attack,
                RootV1::OtherNontokenDeath,
            ] {
                assert_eq!(
                    meaning(&selector, id, root),
                    Err(ErrorV1::UnsupportedDefinition)
                );
            }
            for field in 0..3 {
                let mut forged = selector.clone();
                match field {
                    0 => forged.schema.push('x'),
                    1 => forged.card_db_hash ^= 1,
                    _ => forged.producer_sha256.push('x'),
                }
                assert_eq!(
                    meaning(&forged, id, RootV1::Attack),
                    Err(ErrorV1::WrongSelector)
                );
            }
        } else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog))
        }
    }
}
