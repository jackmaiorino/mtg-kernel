//! Exact Divine spell/kicker/cardinality meaning. No model or sampler admission.
use crate::card_def::{CardType, CombatStaticRulesDef, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::EffectOp;
use crate::mana::{Cost, ManaColor, Pip};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/divine-resilience/v44";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV44 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV44 {
    pub selector: SelectorV44,
    pub card_id: u16,
    pub printed_cost: String,
    pub kicker_cost: String,
    pub unkicked_minimum: u64,
    pub unkicked_maximum: u64,
    pub kicked_minimum: u64,
    pub kicked_maximum: Option<u64>,
    pub distinct_controlled_creature_incarnations: bool,
    pub captured_controller_and_kicker: bool,
    pub each_still_legal_target_gets_indestructible: bool,
    pub cleanup_expires_grants: bool,
    pub zero_target_kicked_resolves: bool,
    pub nonempty_all_illegal_fizzles: bool,
    pub printed_root_sha256: String,
    pub model_sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_divine_v44.rs"),
        include_str!("card_def.rs"),
        include_str!("engine.rs"),
        include_str!("effect.rs"),
        include_str!("../build.rs"),
        include_str!("policy_observation_variable_cast_v1.rs"),
        include_str!("rl_variable_cast_v1.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    format!("{:x}", hash.finalize())
}
pub fn live_selector() -> Result<SelectorV44, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Divine V44 requires the Foundations catalog".into());
    }
    Ok(SelectorV44 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn recipe() -> EffectOp {
    EffectOp::GrantKeywordToDeclaredTargetsUntilEndOfTurn {
        target_spec: TargetSpec::KickerControlledCreatures,
        keyword: Keywords::INDESTRUCTIBLE,
    }
}
pub fn meaning(selector: &SelectorV44, card_id: u16) -> Result<MeaningV44, String> {
    if *selector != live_selector()? {
        return Err("Divine selector differs".into());
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or("Divine definition absent")?;
    let base = Cost {
        generic: 0,
        pips: &[Pip::Colored(ManaColor::W)],
        x_count: 0,
    };
    let kicker = Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::W)],
        x_count: 0,
    };
    if def.name != "Divine Resilience"
        || def.object_name != def.name
        || !def.is_executable()
        || !def.capability.is_fully_supported()
        || def.cost != base
        || def.kicker_cost != Some(kicker)
        || def.mana_value != 1
        || def.types != [CardType::Instant]
        || def.colors != [ManaColor::W]
        || !def.subtypes.is_empty()
        || !def.supertypes.is_empty()
        || (def.power, def.toughness) != (None, None)
        || def.keywords != Keywords::NONE
        || def.target_spec != TargetSpec::KickerControlledCreatures
        || (def.spell_effect)() != Some(recipe())
        || !def.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
        || def.is_token
        || def.is_land
        || def.transform_face.is_some()
        || def.alt_cost.is_some()
        || def.flashback.is_some()
        || def.plot_cost.is_some()
        || def.madness_cost.is_some()
        || def.escape.is_some()
        || def.optional_additional_cost.is_some()
        || def.additional_cost.is_some()
        || def.kicked_target_spec.is_some()
        || def.adventure.is_some()
        || def.mode2.is_some()
        || def.mode3.is_some()
        || !def.additional_modes.is_empty()
        || def.omen.is_some()
        || def.saga.is_some()
        || def.bestow.is_some()
        || def.ward_cost.is_some()
        || def.equipment.is_some()
        || def.generic_cost_reduction.is_some()
        || def.controlled_counter_keyword.is_some()
        || def.controlled_creature_grant.is_some()
        || def.enters_with_plus_one_counters.is_some()
        || def.starting_loyalty.is_some()
        || def.game_outcome_protection_v1.is_some()
        || def.changeling
        || def.spell_cannot_be_countered
        || def.records_mana_spent
        || def.delve
        || def.enters_battlefield_tapped
        || def.enters_battlefield_tapped_unless.is_some()
        || def.enters_tapped_unless_controller.is_some()
        || def.as_enters_choose_color_other_than.is_some()
        || def.attachment.is_some()
        || def.animation.is_some()
        || def.minimum_blockers != 1
        || def.cant_be_blocked_by_monarchs_creatures
        || def.combat_static_rules != CombatStaticRulesDef::NONE
        || def.as_enters_color_choice_v1.is_some()
        || def.chosen_color_creature_boost_v1.is_some()
        || def.damage_replacement_v1.is_some()
        || (def.mana_ability)().is_some()
        || def.mana_ability_def.is_some()
        || !def.produces_mana.is_empty()
        || !def.mana_ability_choices.is_empty()
        || def.mana_ability_includes_chosen_color
        || def.conditional_tap_yield.is_some()
        || !def.additional_mana_abilities.is_empty()
        || !def.additional_mana_ability_conditions.is_empty()
        || !def.restricted_mana_abilities.is_empty()
        || !def.activated_ability_generic_reductions.is_empty()
    {
        return Err("Divine printed definition/root differs".into());
    }
    let bytes = serde_json::to_vec(&(
        card_id,
        def.name,
        def.object_name,
        def.cost,
        def.kicker_cost,
        def.mana_value,
        def.types,
        def.colors,
        def.target_spec,
        recipe(),
        (1_u64, Some(1_u64)),
        (0_u64, None::<u64>),
    ))
    .map_err(|error| error.to_string())?;
    Ok(MeaningV44 {
        selector: selector.clone(),
        card_id,
        printed_cost: "{W}".into(),
        kicker_cost: "{2}{W}".into(),
        unkicked_minimum: 1,
        unkicked_maximum: 1,
        kicked_minimum: 0,
        kicked_maximum: None,
        distinct_controlled_creature_incarnations: true,
        captured_controller_and_kicker: true,
        each_still_legal_target_gets_indestructible: true,
        cleanup_expires_grants: true,
        zero_target_kicked_resolves: true,
        nonempty_all_illegal_fizzles: true,
        printed_root_sha256: format!("{:x}", Sha256::digest(bytes)),
        model_sampler_admitted: false,
    })
}
pub fn validate(record: &MeaningV44) -> Result<(), String> {
    if *record != meaning(&record.selector, record.card_id)? {
        return Err("Divine precise meaning changed".into());
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    #[test]
    fn actual_registered_recipe_and_every_meaning_field_are_rederived() {
        let id = crate::card_def::card_id_by_name("Divine Resilience").unwrap();
        let record = meaning(&live_selector().unwrap(), id).unwrap();
        assert_eq!((record.unkicked_minimum, record.unkicked_maximum), (1, 1));
        assert_eq!((record.kicked_minimum, record.kicked_maximum), (0, None));
        let value = serde_json::to_value(&record).unwrap();
        for (field, old) in value.as_object().unwrap() {
            let replacement = match old {
                serde_json::Value::Bool(value) => serde_json::Value::Bool(!value),
                serde_json::Value::Number(_) => serde_json::json!(99999),
                serde_json::Value::String(_) => serde_json::json!("forged"),
                serde_json::Value::Null => serde_json::json!(2),
                serde_json::Value::Object(_) => {
                    serde_json::json!({"schema":"forged","card_db_hash":0,"producer_sha256":"forged"})
                }
                _ => continue,
            };
            let mut forged = value.clone();
            forged[field] = replacement;
            if let Ok(forged) = serde_json::from_value::<MeaningV44>(forged) {
                assert!(validate(&forged).is_err(), "{field}");
            }
        }
    }
}
