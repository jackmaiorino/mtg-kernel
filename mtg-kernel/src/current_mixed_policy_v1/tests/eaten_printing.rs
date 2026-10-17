//! Exact registered sorcery descriptor and canonical mandatory-cost meaning.
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
        other_controlled_nontoken_death_v1: None,
    }
}

#[test]
fn exact_registered_sorcery_preserves_ordered_mandatory_cost_target_and_exile() {
    let id = crate::card_def::card_id_by_name("Eaten Alive").unwrap();
    let value = printing(id, &recipe(id)).unwrap();
    assert_eq!(registered_spell_printing(id).unwrap(), Some(value.clone()));
    assert!(registered_permanent_printing(id).unwrap().is_none());
    let p = &value["registered_sorcery_printing"];
    assert_eq!(
        p["characteristics"],
        json!({"types":["Sorcery"],"colors":["B"],"mana_value":1,"token":false})
    );
    let root = &p["ordered_cast_roots"][0];
    assert_eq!(p["ordered_cast_roots"].as_array().unwrap().len(), 1);
    assert_eq!(root["method"], "normal");
    assert_eq!(root["ordinary_route"], "Hand");
    assert_eq!(root["ordinary_origin"], "Hand");
    assert_eq!(root["timing"], "CasterMainPhaseEmptyStackWithPriority");
    assert_eq!(root["targets"]["count"], 1);
    assert_eq!(root["targets"]["filter"], "CreatureOrPlaneswalker");
    for key in [
        "may_be_own_or_opposing",
        "bind_original_incarnation_before_payment",
        "recheck_source_aware_legality_at_resolution",
        "departed_or_reentered_target_is_not_rebound",
    ] {
        assert_eq!(root["targets"][key], true);
    }
    let costs = &root["ordered_costs"];
    assert_eq!(
        costs[0],
        json!({"PrintedMana":{"generic":0,"pips":[{"Colored":"B"}],"x_count":0}})
    );
    let branches = &costs[1]["MandatoryOneOf"]["ordered_branches"];
    assert_eq!(branches.as_array().unwrap().len(), 2);
    assert_eq!(
        branches[0],
        json!({"SacrificeCurrentControlledCreature":{"count":1,"zone":"Battlefield","is_target":false,
        "requires_untapped":false,"requires_haste":false,"bind_original_incarnation_before_payment":true,"physical_owner_does_not_select_eligibility":true}})
    );
    assert_eq!(
        branches[1],
        json!({"PayAdditionalMana":{"generic":3,"pips":[{"Colored":"B"}],"x_count":0}})
    );
    for key in [
        "targets_chosen_before_additional_branch",
        "branch_menu_requires_current_native_combined_payment_quote",
        "only_currently_payable_branches_are_offered",
        "sole_payable_branch_selected_automatically",
        "both_payable_branches_use_original_source_bound_native_option_order",
        "multiple_sacrifice_candidates_use_native_cost_target_menu",
        "combined_cost_modifiers_require_current_native_payment_quote",
        "original_source_and_cost_incarnations_committed_before_responses",
        "all_targets_illegal_skips_resolution_without_refund",
        "virtual_spell_copy_ceases",
        "accepted_spell_copy_pays_no_additional_cost",
        "other_cast_permissions_require_current_candidate_authority",
        "zone_replacements_still_apply",
    ] {
        assert_eq!(root[key], true);
    }
    assert_eq!(
        root["ordered_resolution"],
        json!([{"ExileBoundTarget":{"target_slot":0,"requires_same_legal_battlefield_incarnation":true}}])
    );
    assert_eq!(
        root["physical_departure"],
        "GraveyardAfterResolutionOrCounter"
    );
    let meanings: Vec<crate::rules_meaning_eaten_alive_v1::MeaningV1> =
        serde_json::from_value(p["canonical_family_roots"].clone()).unwrap();
    assert_eq!(meanings.len(), 1);
    let meaning = &meanings[0];
    crate::rules_meaning_eaten_alive_v1::validate(meaning).unwrap();
    assert_eq!(meaning.printed_cost, "{B}");
    assert_eq!(meaning.additional_mana_cost, "{3}{B}");
    assert_eq!(meaning.unreduced_mana_branch_total, "{3}{B}{B}");
    let mut forged = meaning.clone();
    forged.additional_branches_ordered.reverse();
    assert!(crate::rules_meaning_eaten_alive_v1::validate(&forged).is_err());
    let base = super::baseline(id, &recipe(id)).unwrap();
    assert_eq!(
        crate::current_mixed_policy_v1::roots::baseline(id).unwrap(),
        base
    );
    assert_eq!(
        base["definition_owned_family_meanings"],
        p["canonical_family_roots"]
    );
    macro_rules! reject {($field:ident,$value:expr)=>{{let mut changed=recipe(id);changed.$field=$value;
        assert!(matches!(printing(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));
        assert!(matches!(super::baseline(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));}};}
    reject!(name, "Llanowar Elves");
    reject!(object_name, "Llanowar Elves");
    reject!(capability, CardCapability::Partial);
    reject!(cost, Cost::zero());
    reject!(mana_value, 5);
    reject!(types, &[CardType::Instant]);
    reject!(colors, &[ManaColor::U]);
    reject!(subtypes, &[crate::card_def::Subtype::Vampire]);
    reject!(supertypes, &[crate::card_def::Supertype::Legendary]);
    reject!(power, Some(1));
    reject!(toughness, Some(1));
    reject!(keywords, Keywords::FLASH);
    reject!(target_spec, TargetSpec::Creature);
    reject!(spell_effect, || Some(EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: Zone::Graveyard
    }));
    reject!(additional_cost, None);
    reject!(additional_cost, Some(crate::eaten_alive_v1::MANA_BRANCH));
    reject!(spell_cannot_be_countered, true);
    reject!(records_mana_spent, true);
    reject!(is_token, true);
    reject!(is_land, true);
    reject!(minimum_blockers, 2);
    reject!(enters_battlefield_tapped, true);
    reject!(produces_mana, &[ManaColor::B]);
    reject!(mana_ability, || Some(EffectOp::Sequence(vec![])));
    reject!(delve, true);
    reject!(changeling, true);
    reject!(
        alt_cost,
        Some(crate::card_def::AltCostDef {
            components: &[],
            condition: crate::card_def::AltCostCondition::Always
        })
    );
    reject!(
        mode2,
        Some(crate::card_def::ModeDef {
            target_spec: TargetSpec::None,
            effect: || EffectOp::Sequence(vec![])
        })
    );
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
