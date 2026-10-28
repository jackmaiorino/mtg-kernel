//! Closed ordinary and additional-kicker printing, not a damage approximation.
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
        kicker_cost: d.kicker_cost,
        additional_cost: d.additional_cost,
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
        creature_type_entry_v1: None,
        other_controlled_nontoken_death_v1: None,
    }
}

#[test]
fn divine_complete_ordinary_and_kicker_printing_preserves_unbounded_target_program() {
    let id = crate::card_def::card_id_by_name("Divine Resilience").unwrap();
    let printed = printing(id, &recipe(id)).unwrap();
    assert_eq!(
        registered_spell_printing(id).unwrap(),
        Some(printed.clone())
    );
    let roots = &printed["registered_instant_printing"]["ordered_cast_roots"];
    assert_eq!(roots.as_array().unwrap().len(), 2);
    let family = meaning(id).unwrap();
    assert_eq!(family.kicked_maximum, None);
    for (index, kicked) in [false, true].into_iter().enumerate() {
        let cast = &roots[index];
        assert_eq!(cast["kicked"], kicked);
        assert_eq!(cast["timing"], "InstantWithPriority");
        assert_eq!(cast["ordinary_origin"], "Hand");
        assert_eq!(cast["ordinary_route"], "Hand");
        assert_eq!(cast["ordered_resolution"], json!([family.clone()]));
        assert_eq!(cast["minimum_targets"], if kicked { 0 } else { 1 });
        assert_eq!(
            cast["maximum_targets"],
            if kicked { Value::Null } else { json!(1) }
        );
        assert_eq!(
            cast["explicit_finish_after_any_kicked_target_prefix"],
            kicked
        );
        assert_eq!(
            cast["physical_departure"],
            "GraveyardAfterResolutionOrCounter"
        );
        assert_eq!(
            cast["ordered_costs"][0],
            json!({"PrintedMana":{"generic":0,"pips":[{"Colored":"W"}],"x_count":0}})
        );
        assert_eq!(
            cast["ordered_costs"].as_array().unwrap().len(),
            if kicked { 2 } else { 1 }
        );
        if kicked {
            assert_eq!(
                cast["ordered_costs"][1],
                json!({"AdditionalKickerMana":{"generic":2,"pips":[{"Colored":"W"}],"x_count":0}})
            );
        }
    }
    assert_eq!(
        super::baseline(id, &recipe(id)).unwrap()["definition_owned_family_meanings"],
        json!([family])
    );
    macro_rules! reject {
        ($field:ident,$value:expr) => {{
            let mut d = recipe(id);
            d.$field = $value;
            assert!(validate_shape(id, &d).is_err(), stringify!($field));
            assert!(printing(id, &d).is_err(), stringify!($field));
            assert!(super::baseline(id, &d).is_err(), stringify!($field));
        }};
    }
    reject!(name, "Lightning Bolt");
    reject!(object_name, "Lightning Bolt");
    reject!(capability, CardCapability::Partial);
    reject!(cost, Cost::zero());
    reject!(kicker_cost, None);
    reject!(
        kicker_cost,
        Some(Cost {
            generic: 1,
            pips: &[],
            x_count: 0
        })
    );
    reject!(mana_value, 4);
    reject!(types, &[CardType::Sorcery]);
    reject!(subtypes, &[crate::card_def::Subtype::Vampire]);
    reject!(supertypes, &[crate::card_def::Supertype::Legendary]);
    reject!(colors, &[ManaColor::R]);
    reject!(power, Some(1));
    reject!(toughness, Some(1));
    reject!(keywords, Keywords::FLASH);
    reject!(target_spec, TargetSpec::Creature);
    reject!(spell_effect, || Some(EffectOp::Sequence(vec![])));
    reject!(additional_cost, Some(crate::eaten_alive_v1::ADDITIONAL));
    reject!(spell_cannot_be_countered, true);
    reject!(records_mana_spent, true);
    reject!(is_token, true);
    reject!(is_land, true);
    reject!(enters_battlefield_tapped, true);
    reject!(minimum_blockers, 2);
    reject!(produces_mana, &[ManaColor::W]);
    reject!(mana_ability, || Some(EffectOp::Sequence(vec![])));
    reject!(changeling, true);
    reject!(delve, true);
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
}
