//! Precise Full Ajani roots. Partial recipient, vectors and weights remain distinct.
use crate::card_def::{DynamicValueDef, CARD_DEFS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/ajani/v42";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV42 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV42 {
    PrintedPlaneswalkerSpell,
    PlusOneCounter,
    MinusThreeKeywords,
    MinusEightCats,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepresentationV42 {
    ExactAjaniV42,
    FrozenRulesVectorV1,
    FrozenModelFeaturesV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV42 {
    PrintedPlaneswalkerSpell {
        printed_cost_one_generic_two_white: bool,
        legendary_white_ajani_starting_four: bool,
        ordinary_cast_permissions_and_modifiers_apply: bool,
        ordinary_untargeted_battlefield_entry: bool,
    },
    PlusOneCounter {
        loyalty_cost: i32,
        minimum_targets: u8,
        maximum_targets: u8,
        any_creature_not_only_controlled: bool,
        zero_targets_still_pays_loyalty_and_use: bool,
        one_plus_one_counter_on_legal_exact_incarnation: bool,
        counter_modifying_effects_apply: bool,
    },
    MinusThreeKeywords {
        loyalty_cost: i32,
        exactly_one_creature_target: bool,
        flying_and_double_strike_until_cleanup: bool,
        source_departure_or_control_change_does_not_end_grant: bool,
        grant_does_not_follow_target_reentry: bool,
    },
    MinusEightCats {
        loyalty_cost: i32,
        no_targets: bool,
        count: DynamicValueDef,
        sample_captured_controller_current_life_at_resolution: bool,
        source_departure_does_not_change_token_owner: bool,
        zero_life_creates_zero_tokens: bool,
        two_two_white_cat_tokens_no_abilities: bool,
        one_simultaneous_complete_entry_batch_before_observers_and_sba: bool,
        no_arbitrary_token_cap_or_narrow_count: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV42 {
    pub selector: SelectorV42,
    pub card_id: u16,
    pub root: RootV42,
    pub root_sha256: String,
    pub rule: RuleV42,
    pub shared_once_per_turn_across_all_loyalty_abilities: bool,
    pub sorcery_timing_and_current_source_abilities_required: bool,
    pub captured_source_controller_color_and_target_incarnation_rechecked: bool,
    pub countered_or_all_illegal_target_ability_keeps_cost_paid: bool,
    pub native_catalog_model_sampler_admitted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV42 {
    UnsupportedCatalog,
    UnsupportedRepresentation,
    UnsupportedDefinition,
    WrongSelector,
    ForgedMeaning,
}
fn digest(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_ajani_v42.rs"),
        include_str!("ajani_full_v42.rs"),
        include_str!("ajani_full_v42/lifecycle.rs"),
        include_str!("engine.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("planeswalker_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("mana.rs"),
        include_str!("trigger.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("controlled_creature_grants_v1.rs"),
        include_str!("garruks_uprising_v1.rs"),
        include_str!("arahbo_v1.rs"),
        include_str!("authority_of_the_consuls_v1.rs"),
        include_str!("raid_entry_v1.rs"),
        include_str!("../build.rs"),
    ] {
        h.update([0]);
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector() -> Result<SelectorV42, ErrorV42> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV42::UnsupportedCatalog);
    }
    Ok(SelectorV42 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(selector: &SelectorV42, id: u16, root: RootV42) -> Result<MeaningV42, ErrorV42> {
    if *selector != live_selector()? {
        return Err(ErrorV42::WrongSelector);
    }
    crate::ajani_full_v42::validate_definition(id).map_err(|_| ErrorV42::UnsupportedDefinition)?;
    let token =
        crate::ajani_full_v42::token_definition().map_err(|_| ErrorV42::UnsupportedDefinition)?;
    let rule = match root {
        RootV42::PrintedPlaneswalkerSpell => RuleV42::PrintedPlaneswalkerSpell {
            printed_cost_one_generic_two_white: true,
            legendary_white_ajani_starting_four: true,
            ordinary_cast_permissions_and_modifiers_apply: true,
            ordinary_untargeted_battlefield_entry: true,
        },
        RootV42::PlusOneCounter => RuleV42::PlusOneCounter {
            loyalty_cost: 1,
            minimum_targets: 0,
            maximum_targets: 1,
            any_creature_not_only_controlled: true,
            zero_targets_still_pays_loyalty_and_use: true,
            one_plus_one_counter_on_legal_exact_incarnation: true,
            counter_modifying_effects_apply: true,
        },
        RootV42::MinusThreeKeywords => RuleV42::MinusThreeKeywords {
            loyalty_cost: -3,
            exactly_one_creature_target: true,
            flying_and_double_strike_until_cleanup: true,
            source_departure_or_control_change_does_not_end_grant: true,
            grant_does_not_follow_target_reentry: true,
        },
        RootV42::MinusEightCats => RuleV42::MinusEightCats {
            loyalty_cost: -8,
            no_targets: true,
            count: crate::ajani_full_v42::LIFE_COUNT,
            sample_captured_controller_current_life_at_resolution: true,
            source_departure_does_not_change_token_owner: true,
            zero_life_creates_zero_tokens: true,
            two_two_white_cat_tokens_no_abilities: true,
            one_simultaneous_complete_entry_batch_before_observers_and_sba: true,
            no_arbitrary_token_cap_or_narrow_count: true,
        },
    };
    let d = &CARD_DEFS[usize::from(id)];
    let t = &CARD_DEFS[usize::from(token)];
    // Frozen Supertype has no Serialize. Its checked printed value is explicit.
    let material = serde_json::json!({"id":id,"root":root,"cost":d.cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,"supertypes":["Legendary"],"colors":d.colors,"starting_loyalty":d.starting_loyalty,"spell":(d.spell_effect)(),"abilities":d.activated_abilities.iter().map(|a|(a.cost,a.target_spec,(a.effect)(),a.activation_zone,a.sorcery_speed_only)).collect::<Vec<_>>(),"token":{"id":token,"name":t.object_name,"types":t.types,"subtypes":t.subtypes,"colors":t.colors,"power":t.power,"toughness":t.toughness,"keywords":t.keywords},"rule":rule});
    Ok(MeaningV42 {
        selector: selector.clone(),
        card_id: id,
        root,
        root_sha256: digest(serde_json::to_vec(&material).expect("typed Ajani meaning serializes")),
        rule,
        shared_once_per_turn_across_all_loyalty_abilities: true,
        sorcery_timing_and_current_source_abilities_required: true,
        captured_source_controller_color_and_target_incarnation_rechecked: true,
        countered_or_all_illegal_target_ability_keeps_cost_paid: true,
        native_catalog_model_sampler_admitted: false,
    })
}
pub fn meaning_for_representation(
    representation: RepresentationV42,
    selector: &SelectorV42,
    id: u16,
    root: RootV42,
) -> Result<MeaningV42, ErrorV42> {
    if representation != RepresentationV42::ExactAjaniV42 {
        return Err(ErrorV42::UnsupportedRepresentation);
    }
    meaning(selector, id, root)
}
pub fn validate(value: &MeaningV42) -> Result<(), ErrorV42> {
    if *value != meaning(&value.selector, value.card_id, value.root)? {
        return Err(ErrorV42::ForgedMeaning);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_partial_or_other_catalog_does_not_fabricate_full_meaning() {
        if !cfg!(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )) {
            assert_eq!(live_selector(), Err(ErrorV42::UnsupportedCatalog));
            return;
        }
        if let Some(id) = crate::card_def::card_id_by_name("Ajani, Caller of the Pride") {
            if CARD_DEFS[usize::from(id)].capability == crate::card_def::CardCapability::Partial {
                assert_eq!(
                    meaning(&live_selector().unwrap(), id, RootV42::PlusOneCounter),
                    Err(ErrorV42::UnsupportedDefinition)
                );
            }
        }
    }
    #[test]
    #[ignore = "requires genuine Full Ajani and its distinct 2/2 Cat token"]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_four_roots_reconstruct_all_facts_and_refuse_frozen_formats() {
        let id = crate::card_def::card_id_by_name("Ajani, Caller of the Pride").unwrap();
        let selector = live_selector().unwrap();
        for root in [
            RootV42::PrintedPlaneswalkerSpell,
            RootV42::PlusOneCounter,
            RootV42::MinusThreeKeywords,
            RootV42::MinusEightCats,
        ] {
            let actual = meaning(&selector, id, root).unwrap();
            validate(&actual).unwrap();
            for representation in [
                RepresentationV42::FrozenRulesVectorV1,
                RepresentationV42::FrozenModelFeaturesV1,
            ] {
                assert_eq!(
                    meaning_for_representation(representation, &selector, id, root),
                    Err(ErrorV42::UnsupportedRepresentation)
                );
            }
            let mut forged = actual.clone();
            forged.root_sha256.clear();
            assert_eq!(validate(&forged), Err(ErrorV42::ForgedMeaning));
            let mut forged = actual.clone();
            forged.native_catalog_model_sampler_admitted = true;
            assert_eq!(validate(&forged), Err(ErrorV42::ForgedMeaning));
            let mut forged = actual.clone();
            forged.selector.producer_sha256.clear();
            assert_eq!(validate(&forged), Err(ErrorV42::WrongSelector));
            let value = serde_json::to_value(&actual).unwrap();
            let (kind, fields) = value["rule"].as_object().unwrap().iter().next().unwrap();
            for (name, field) in fields.as_object().unwrap() {
                let mut altered = value.clone();
                altered["rule"][kind][name] = match field {
                    serde_json::Value::Bool(_) => serde_json::Value::Bool(false),
                    serde_json::Value::Number(_) => serde_json::Value::Number(999.into()),
                    _ => serde_json::Value::String("forged".into()),
                };
                if let Ok(forged) = serde_json::from_value::<MeaningV42>(altered) {
                    assert!(validate(&forged).is_err(), "{name}");
                }
            }
        }
    }
}
