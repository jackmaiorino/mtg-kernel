//! Selected printed Krenko roots, separate from frozen vectors and model admission.
use crate::card_def::{
    ActivationTargetFilter, CardType, CostComponent, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/krenko/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    PrintedCreatureSpell,
    PrintedTapActivation { ability_index: u16 },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV1 {
    PrintedCreatureSpell {
        printed_cost_two_generic_two_red: bool,
        legendary_red_goblin_warrior_three_three: bool,
        ordinary_cast_permissions_and_modifiers_apply: bool,
        ordinary_untargeted_battlefield_entry: bool,
    },
    PrintedTapActivation {
        tap_original_source_as_full_cost: bool,
        creature_tap_sickness_haste_and_suppression_apply: bool,
        instant_speed_no_targets_no_once_limit: bool,
        count_controlled_goblin_permanents_not_only_creatures: bool,
        count_at_resolution_includes_source_if_still_controlled_goblin: bool,
        counted_tokens_copied_permanents_and_effective_subtypes_participate: bool,
        captured_controller_owns_tokens_even_after_source_control_or_zone_change: bool,
        no_arbitrary_token_cap_or_narrow_count_transport: bool,
        one_simultaneous_complete_token_entry_batch_before_observers_and_sba: bool,
        one_one_red_goblin_tokens_no_extra_keywords: bool,
        actual_payment_private_removal_and_token_occurrences_bind_completion: bool,
        countered_activation_creates_no_tokens_and_keeps_cost_paid: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub model_sampler_and_catalog_admitted: bool,
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
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_krenko_v1.rs"),
        include_str!("krenko_v1.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("trigger.rs"),
        include_str!("mana.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("garruks_uprising_v1.rs"),
        include_str!("arahbo_v1.rs"),
        include_str!("authority_of_the_consuls_v1.rs"),
        include_str!("raid_entry_v1.rs"),
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
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(selector: &SelectorV1, id: u16, root: RootV1) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if !crate::krenko_v1::selected_definition(id) || !d.has_full_support() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let cost = Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    let ordinary = Some(EffectOp::MoveObject {
        object: ObjectRef::ThisSource,
        to_zone: Zone::Battlefield,
    });
    if d.cost != cost
        || d.object_name != d.name
        || d.mana_value != 4
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Goblin, Subtype::Warrior]
        || d.supertypes != [Supertype::Legendary]
        || d.colors != [ManaColor::R]
        || (d.power, d.toughness) != (Some(3), Some(3))
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)() != ordinary
        || !crate::trigger::triggers_for(id).is_empty()
        || d.is_token
        || d.is_land
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || d.mana_ability_def.is_some()
        || !d.additional_mana_abilities.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.alt_cost.is_some()
        || d.additional_cost.is_some()
        || d.optional_additional_cost.is_some()
        || d.kicker_cost.is_some()
        || d.kicked_target_spec.is_some()
        || d.flashback.is_some()
        || d.escape.is_some()
        || d.madness_cost.is_some()
        || d.plot_cost.is_some()
        || d.bestow.is_some()
        || d.omen.is_some()
        || d.adventure.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.transform_face.is_some()
        || d.attachment.is_some()
        || d.equipment.is_some()
        || d.animation.is_some()
        || d.saga.is_some()
        || d.changeling
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant.is_some()
        || d.enters_battlefield_tapped
        || d.enters_battlefield_tapped_unless.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || d.starting_loyalty.is_some()
        || d.ward_cost.is_some()
        || d.generic_cost_reduction.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.delve
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || d.conditional_tap_yield.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.minimum_blockers > 1
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
    {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let [ability] = d.activated_abilities else {
        return Err(ErrorV1::UnsupportedDefinition);
    };
    if ability.cost != [CostComponent::Tap]
        || ability.target_spec != TargetSpec::None
        || (ability.effect)() != crate::krenko_v1::program()
        || ability.sorcery_speed_only
        || ability.activation_zone != Zone::Battlefield
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.face.is_some()
        || ability.max_activations_per_turn.is_some()
    {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let token = crate::krenko_v1::token_definition().map_err(|_| ErrorV1::UnsupportedDefinition)?;
    let rule = match root {
        RootV1::PrintedCreatureSpell => RuleV1::PrintedCreatureSpell {
            printed_cost_two_generic_two_red: true,
            legendary_red_goblin_warrior_three_three: true,
            ordinary_cast_permissions_and_modifiers_apply: true,
            ordinary_untargeted_battlefield_entry: true,
        },
        RootV1::PrintedTapActivation { ability_index: 0 } => RuleV1::PrintedTapActivation {
            tap_original_source_as_full_cost: true,
            creature_tap_sickness_haste_and_suppression_apply: true,
            instant_speed_no_targets_no_once_limit: true,
            count_controlled_goblin_permanents_not_only_creatures: true,
            count_at_resolution_includes_source_if_still_controlled_goblin: true,
            counted_tokens_copied_permanents_and_effective_subtypes_participate: true,
            captured_controller_owns_tokens_even_after_source_control_or_zone_change: true,
            no_arbitrary_token_cap_or_narrow_count_transport: true,
            one_simultaneous_complete_token_entry_batch_before_observers_and_sba: true,
            one_one_red_goblin_tokens_no_extra_keywords: true,
            actual_payment_private_removal_and_token_occurrences_bind_completion: true,
            countered_activation_creates_no_tokens_and_keeps_cost_paid: true,
        },
        _ => return Err(ErrorV1::WrongRoot),
    };
    let bytes = serde_json::to_vec(&(
        id,
        root,
        d.cost,
        d.mana_value,
        d.types,
        d.subtypes,
        "Legendary",
        d.colors,
        d.power,
        d.toughness,
        d.keywords,
        (d.spell_effect)(),
        crate::krenko_v1::COUNT,
        (ability.effect)(),
        token,
        &rule,
    ))
    .expect("typed printed root serializes");
    let root_sha256 = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id: id,
        root,
        root_sha256,
        rule,
        model_sampler_and_catalog_admitted: false,
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
    fn absent_selected_definition_and_predecessor_catalog_do_not_fabricate_root() {
        if !cfg!(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )) {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
        } else if crate::card_def::card_id_by_name("Krenko, Mob Boss").is_none() {
            let selector = live_selector().unwrap();
            assert_eq!(
                meaning(
                    &selector,
                    crate::card_def::card_id_by_name("Goblin Token").unwrap(),
                    RootV1::PrintedCreatureSpell
                ),
                Err(ErrorV1::UnsupportedDefinition)
            );
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_two_roots_reconstruct_every_fact_and_reject_selectors_or_coherent_rewrites() {
        let id = crate::card_def::card_id_by_name("Krenko, Mob Boss").unwrap();
        let selector = live_selector().unwrap();
        for root in [
            RootV1::PrintedCreatureSpell,
            RootV1::PrintedTapActivation { ability_index: 0 },
        ] {
            let actual = meaning(&selector, id, root).unwrap();
            validate(&actual).unwrap();
            let mut forged = actual.clone();
            forged.root_sha256.clear();
            assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch));
            let mut forged = actual.clone();
            forged.model_sampler_and_catalog_admitted = true;
            assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch));
            let mut forged = actual.clone();
            forged.selector.producer_sha256.clear();
            assert_eq!(validate(&forged), Err(ErrorV1::WrongSelector));
            let value = serde_json::to_value(&actual).unwrap();
            let rule = value["rule"].as_object().unwrap();
            let (variant, fields) = rule.iter().next().unwrap();
            for field in fields.as_object().unwrap().keys() {
                let mut forged = value.clone();
                forged["rule"][variant][field] = serde_json::Value::Bool(false);
                let forged: MeaningV1 = serde_json::from_value(forged).unwrap();
                assert_eq!(validate(&forged), Err(ErrorV1::IdentityMismatch), "{field}");
            }
        }
        assert_eq!(
            meaning(
                &selector,
                id,
                RootV1::PrintedTapActivation { ability_index: 1 }
            ),
            Err(ErrorV1::WrongRoot)
        );
        assert_eq!(
            meaning(
                &selector,
                crate::card_def::card_id_by_name("Goblin Token").unwrap(),
                RootV1::PrintedCreatureSpell
            ),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
}
