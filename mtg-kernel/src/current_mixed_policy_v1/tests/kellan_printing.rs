//! Exact printing/ability closure and deterministic private typed canonical roots.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use super::*;
fn recipe(id: u16) -> crate::card_def::CardDef {
    let d = &crate::card_def::CARD_DEFS[usize::from(id)];
    crate::card_def::CardDef {
        name: d.name,
        capability: d.capability,
        cost: d.cost,
        generic_cost_reduction: None,
        ward_cost: None,
        spell_cannot_be_countered: d.spell_cannot_be_countered,
        records_mana_spent: d.records_mana_spent,
        equipment: None,
        types: d.types,
        subtypes: d.subtypes,
        supertypes: d.supertypes,
        power: d.power,
        toughness: d.toughness,
        starting_loyalty: None,
        enters_with_plus_one_counters: None,
        controlled_counter_keyword: None,
        game_outcome_protection_v1: d.game_outcome_protection_v1,
        controlled_creature_grant: d.controlled_creature_grant,
        is_land: d.is_land,
        produces_mana: d.produces_mana,
        colors: d.colors,
        target_spec: d.target_spec,
        keywords: d.keywords,
        spell_effect: d.spell_effect,
        mana_ability: d.mana_ability,
        alt_cost: None,
        kicker_cost: None,
        additional_cost: None,
        flashback: None,
        activated_abilities: d.activated_abilities,
        plot_cost: None,
        madness_cost: None,
        mode2: None,
        mode3: None,
        additional_modes: d.additional_modes,
        kicked_target_spec: None,
        is_token: d.is_token,
        escape: None,
        mana_ability_choices: d.mana_ability_choices,
        enters_battlefield_tapped: d.enters_battlefield_tapped,
        mana_ability_def: None,
        minimum_blockers: d.minimum_blockers,
        omen: None,
        mana_value: d.mana_value,
        mana_ability_includes_chosen_color: d.mana_ability_includes_chosen_color,
        as_enters_choose_color_other_than: None,
        additional_mana_abilities: d.additional_mana_abilities,
        object_name: d.object_name,
        enters_battlefield_tapped_unless: None,
        attachment: None,
        transform_face: None,
        saga: None,
        optional_additional_cost: None,
        changeling: d.changeling,
        bestow: None,
        delve: d.delve,
        adventure: None,
        cant_be_blocked_by_monarchs_creatures: d.cant_be_blocked_by_monarchs_creatures,
        conditional_tap_yield: None,
        enters_tapped_unless_controller: None,
        additional_mana_ability_conditions: d.additional_mana_ability_conditions,
        restricted_mana_abilities: d.restricted_mana_abilities,
        animation: None,
        activated_ability_generic_reductions: d.activated_ability_generic_reductions,
        combat_static_rules: d.combat_static_rules,
        as_enters_color_choice_v1: d.as_enters_color_choice_v1,
        chosen_color_creature_boost_v1: d.chosen_color_creature_boost_v1,
        damage_replacement_v1: d.damage_replacement_v1,
        token_counter_replacement_v48: None,
        starting_life_self_bonuses_v1: d.starting_life_self_bonuses_v1,
        source_type_hexproof_v1: None,
        other_controlled_nontoken_death_v1: None,
    }
}

#[test]
fn kellan_closed_printing_preserves_both_resolution_conditions_costs_layers_and_impulse_window() {
    let id = crate::card_def::card_id_by_name("Kellan, Planar Trailblazer").unwrap();
    let value = printing(id, &recipe(id)).unwrap().unwrap();
    assert_eq!(
        registered_permanent_printing(id).unwrap(),
        Some(value.clone())
    );
    let p = &value["registered_permanent_printing"];
    let c = &p["characteristics"];
    assert_eq!(c["types"], json!(["Creature"]));
    assert_eq!(c["subtypes"], json!(["Human", "Faerie", "Scout"]));
    assert_eq!(
        c["supertypes"],
        json!([crate::card_def::Supertype::Legendary as u8])
    );
    assert_eq!(c["colors"], json!(["R"]));
    assert_eq!(c["mana_value"], 1);
    assert_eq!(c["power"], 2);
    assert_eq!(c["toughness"], 1);
    assert_eq!(c["ordered_keywords"], json!([]));
    assert_eq!(c["printed_activated_abilities"], 2);
    assert_eq!(c["printed_mana_abilities"], 0);
    let cast = &p["ordered_cast_roots"][0];
    assert_eq!(p["ordered_cast_roots"].as_array().unwrap().len(), 1);
    assert_eq!(cast["method"], "normal");
    assert_eq!(cast["ordinary_route"], "Hand");
    assert_eq!(cast["ordinary_origin"], "Hand");
    assert_eq!(cast["timing"], "CasterMainPhaseEmptyStackWithPriority");
    assert_eq!(cast["targets"], "None");
    assert_eq!(
        cast["ordered_costs"],
        json!([{"PrintedMana":{"generic":0,"pips":[{"Colored":"R"}],"x_count":0}}])
    );
    let root = serde_json::to_value(meaning(id).unwrap()).unwrap();
    assert_eq!(p["canonical_family_roots"], json!([root.clone()]));
    let base = super::baseline(id, &recipe(id)).unwrap();
    assert_eq!(
        crate::current_mixed_policy_v1::roots::baseline(id).unwrap(),
        base
    );
    assert_eq!(
        base["definition_owned_family_meanings"],
        p["canonical_family_roots"]
    );
    let upgrades = root["ordered_upgrade_roots"].as_array().unwrap();
    assert_eq!(upgrades.len(), 2);
    for (index, ability) in upgrades.iter().enumerate() {
        assert_eq!(ability["printed_ability_index"], index);
        assert_eq!(ability["target_count"], 0);
        assert_eq!(ability["activation_zone"], "Battlefield");
        assert_eq!(ability["requires_tap_or_haste"], false);
        assert_eq!(
            ability["prerequisite_effective_subtype_at_resolution"],
            if index == 0 { "Scout" } else { "Detective" }
        );
        assert_eq!(
            ability["ordered_costs"],
            json!([{"PrintedMana":{"generic":index+1,"pips":[{"Colored":"R"}],"x_count":0}}])
        );
        for key in [
            "current_source_controller_must_be_actor",
            "instant_speed_with_priority",
            "requires_active_effective_printed_slot",
            "uses_stack_and_can_be_countered",
            "prerequisite_is_not_activation_restriction",
            "failed_condition_or_departed_source_keeps_payment_without_effect",
            "original_source_incarnation_bound_before_payment",
            "timestamp_ordered_layers",
            "persists_through_cleanup",
            "source_departure_ends_upgrade_and_reentry_starts_printed",
            "rogue_upgrade_preserves_previously_granted_combat_impulse",
        ] {
            assert_eq!(ability[key], true);
        }
        let steps = ability["ordered_resolution"].as_array().unwrap();
        assert_eq!(
            steps[0],
            json!({"ReplaceCreatureSubtypes":{"types":["Human","Faerie",if index==0 {"Detective"} else {"Rogue"}],"preserve_noncreature_subtypes":true,"layer":"TypeChanging4"}})
        );
        if index == 0 {
            assert_eq!(steps.len(), 2);
            assert_eq!(
                steps[1],
                json!({"GrantCombatDamagePlayerImpulse":{"layer":"AbilityAdding6"}})
            );
        } else {
            assert_eq!(steps.len(), 3);
            assert_eq!(
                steps[1],
                json!({"SetBasePowerToughness":{"power":3,"toughness":2,"layer":"BasePowerToughness7b"}})
            );
            assert_eq!(
                steps[2],
                json!({"GrantDoubleStrike":{"layer":"AbilityAdding6"}})
            );
        }
    }
    let impulse = &root["granted_combat_impulse"];
    assert_eq!(impulse["native_definition_trigger_index"], 0);
    assert_eq!(impulse["target_count"], 0);
    assert_eq!(impulse["exile_top_cards"], 1);
    for (key, value) in impulse.as_object().unwrap() {
        if value.is_boolean() {
            assert_eq!(value.as_bool(), Some(true), "{key}");
        }
    }
    // Canonical construction is definition-owned. Altered ability programs or
    // printing fields refuse before any public descriptor is returned.
    macro_rules! reject {($field:ident,$value:expr)=>{{let mut d=recipe(id);d.$field=$value;
        assert!(matches!(printing(id,&d),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));
        assert!(matches!(super::baseline(id,&d),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));}};}
    reject!(name, "Llanowar Elves");
    reject!(object_name, "Llanowar Elves");
    reject!(capability, CardCapability::Partial);
    reject!(cost, Cost::zero());
    reject!(mana_value, 3);
    reject!(types, &[CardType::Enchantment]);
    reject!(subtypes, &[Subtype::Human]);
    reject!(supertypes, &[]);
    reject!(colors, &[ManaColor::G]);
    reject!(power, Some(3));
    reject!(toughness, Some(2));
    reject!(keywords, Keywords::DOUBLE_STRIKE);
    reject!(activated_abilities, &[]);
    reject!(target_spec, TargetSpec::Creature);
    reject!(spell_effect, || None);
    reject!(additional_cost, Some(crate::eaten_alive_v1::ADDITIONAL));
    reject!(produces_mana, &[ManaColor::R]);
    reject!(mana_ability, || Some(EffectOp::Sequence(vec![])));
    reject!(spell_cannot_be_countered, true);
    reject!(records_mana_spent, true);
    reject!(is_token, true);
    reject!(is_land, true);
    reject!(enters_battlefield_tapped, true);
    reject!(delve, true);
    reject!(changeling, true);
    reject!(minimum_blockers, 2);
    reject!(
        source_type_hexproof_v1,
        Some(crate::card_def::SourceTypeHexproofDefV1::Instants)
    );
    reject!(
        other_controlled_nontoken_death_v1,
        Some(
            crate::card_def::OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom
        )
    );
    let foreign = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
    assert!(printing(foreign, &recipe(id)).is_err());
}
