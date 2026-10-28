//! Exact producer dispatch and complete Fiery printing closure.
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
fn actual_fiery_root_transport_and_closed_ordinary_instant() {
    let id = crate::card_def::card_id_by_name("Fiery Annihilation").unwrap();
    let family = meaning(id).unwrap();
    let row = serde_json::to_value(crate::fiery_observation_v1::RootV1 {
        card_id: id,
        spell_mode: 0,
        root_sha256: family.root_sha256.clone(),
    })
    .unwrap();
    assert_eq!(
        expand_thin(row.as_object().unwrap()).unwrap(),
        serde_json::to_value(&family).unwrap()
    );
    for bad in [
        json!({"card_id":id,"root_sha256":family.root_sha256}),
        json!({"card_id":id,"spell_mode":1,"root_sha256":family.root_sha256}),
        json!({"card_id":id,"spell_mode":0,"root":"PrintedSpell","root_sha256":family.root_sha256}),
        json!({"card_id":id,"spell_mode":0,"root_sha256":"forged"}),
    ] {
        assert!(
            matches!(expand_thin(bad.as_object().unwrap()),Err(Error::UnsupportedPrintedRoot(actual)) if actual==id)
        );
    }
    let value = printing(id, &recipe(id)).unwrap();
    assert_eq!(registered_spell_printing(id).unwrap(), Some(value.clone()));
    let cast = &value["registered_instant_printing"]["ordered_cast_roots"][0];
    assert_eq!(cast["timing"], "InstantWithPriority");
    assert_eq!(cast["ordinary_origin"], "Hand");
    assert_eq!(cast["mandatory_creature_targets"], 1);
    assert_eq!(cast["optional_attached_equipment_minimum"], 0);
    assert_eq!(cast["optional_attached_equipment_maximum"], 1);
    assert_eq!(
        cast["ordered_costs"],
        json!([{"PrintedMana":{"generic":2,"pips":[{"Colored":"R"}],"x_count":0}}])
    );
    assert_eq!(cast["ordered_resolution"], json!([family]));
    assert_eq!(
        cast["physical_departure"],
        "GraveyardAfterResolutionOrCounter"
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
    reject!(mana_value, 1);
    reject!(types, &[CardType::Sorcery]);
    reject!(colors, &[ManaColor::U]);
    reject!(subtypes, &[crate::card_def::Subtype::Vampire]);
    reject!(supertypes, &[crate::card_def::Supertype::Legendary]);
    reject!(power, Some(1));
    reject!(toughness, Some(1));
    reject!(keywords, Keywords::FLASH);
    reject!(target_spec, TargetSpec::Creature);
    reject!(is_land, true);
    reject!(is_token, true);
    reject!(spell_effect, || Some(EffectOp::DealDamage {
        target: crate::effect::TargetRef::Target(0),
        amount: 5
    }));
    reject!(spell_cannot_be_countered, true);
    reject!(records_mana_spent, true);
    reject!(additional_cost, Some(crate::eaten_alive_v1::ADDITIONAL));
    reject!(enters_battlefield_tapped, true);
    reject!(minimum_blockers, 2);
    reject!(produces_mana, &[ManaColor::R]);
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
