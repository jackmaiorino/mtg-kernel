//! Closed spell/token definitions, two exact canonical roots and ordered
//! creation/reflexive semantics. Runtime ownership paths live in ordinary_triggers.
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
fn complete_instant_and_faerie_blueprint_preserve_creation_then_distinct_reflexive() {
    let id = crate::card_def::card_id_by_name("Faebloom Trick").unwrap();
    let value = printing(id, &recipe(id)).unwrap();
    assert_eq!(registered_spell_printing(id).unwrap(), Some(value.clone()));
    assert!(registered_permanent_printing(id).unwrap().is_none());
    let p = &value["registered_instant_printing"];
    assert_eq!(
        p["characteristics"],
        json!({"types":["Instant"],"colors":["U"],"mana_value":3,"token":false})
    );
    let cast = &p["ordered_cast_roots"][0];
    assert_eq!(p["ordered_cast_roots"].as_array().unwrap().len(), 1);
    assert_eq!(cast["method"], "normal");
    assert_eq!(cast["ordinary_route"], "Hand");
    assert_eq!(cast["ordinary_origin"], "Hand");
    assert_eq!(cast["timing"], "InstantWithPriority");
    assert_eq!(cast["target_count"], 0);
    assert_eq!(cast["targets"], "None");
    assert_eq!(
        cast["ordered_costs"],
        json!([{"PrintedMana":{"generic":2,"pips":[{"Colored":"U"}],"x_count":0}}])
    );
    let create = &cast["ordered_resolution"][0]["CreateTwoFaeriesThenCaptureOneReflexive"];
    assert_eq!(cast["ordered_resolution"].as_array().unwrap().len(), 1);
    assert_eq!(create["proposed_token_count"], 2);
    for key in [
        "actual_native_token_replacements_and_entry_batch_apply",
        "capture_requires_actual_committed_creation",
        "replacement_token_count_does_not_multiply_reflexive_occurrences",
        "each_accepted_spell_copy_has_distinct_creation_and_reflexive_controller",
    ] {
        assert_eq!(create[key], true);
    }
    let token_id = crate::card_def::card_id_by_name("Faerie Token").unwrap();
    assert_eq!(create["token_blueprint"]["card_id"], token_id);
    assert_eq!(create["token_blueprint"]["no_ordinary_cast_route"], true);
    let token = &create["token_blueprint"]["characteristics"];
    assert_eq!(token["types"], json!(["Creature"]));
    assert_eq!(token["subtypes"], json!(["Faerie"]));
    assert_eq!(token["supertypes"], json!([]));
    assert_eq!(token["colors"], json!(["U"]));
    assert_eq!(token["power"], 1);
    assert_eq!(token["toughness"], 1);
    assert_eq!(token["mana_value"], 0);
    assert_eq!(token["ordered_keywords"], json!(["Flying"]));
    assert_eq!(token["minimum_blockers"], 1);
    assert_eq!(token["token"], true);
    for key in ["land", "enters_tapped"] {
        assert_eq!(token[key], false);
    }
    for key in ["printed_mana_abilities", "printed_activated_abilities"] {
        assert_eq!(token[key], 0);
    }
    for key in [
        "payment_precedes_finalized_spell_and_responses",
        "virtual_spell_copy_ceases",
        "countered_spell_creates_no_tokens_and_no_reflexive",
        "other_cast_permissions_require_current_candidate_authority",
        "zone_replacements_still_apply",
    ] {
        assert_eq!(cast[key], true);
    }
    assert_eq!(
        cast["physical_departure"],
        "GraveyardAfterResolutionOrCounter"
    );
    let reflexive = &p["ordered_reflexive_roots"][0];
    assert_eq!(p["ordered_reflexive_roots"].as_array().unwrap().len(), 1);
    assert_eq!(reflexive["target_count"], 1);
    assert_eq!(reflexive["target_filter"], "OpponentControlledCreature");
    assert_eq!(reflexive["current_source_survival_required"], false);
    for key in [
        "mandatory",
        "uses_separate_stack_item",
        "target_selected_only_after_actual_creation",
        "already_tapped_target_is_legal",
        "controller_and_blue_source_bound_to_original_spell_occurrence",
        "original_target_incarnation_and_source_aware_legality_rechecked_at_resolution",
        "no_legal_target_drops_reflexive_without_undoing_creation",
        "ordinary_apnap_placement_priority_responses_and_counterability",
        "native_original_occurrence_owns_publication_and_once_only_completion",
    ] {
        assert_eq!(reflexive[key], true);
    }
    use crate::rules_meaning_faebloom_v43 as canonical;
    let meanings: Vec<canonical::MeaningV43> =
        serde_json::from_value(p["canonical_family_roots"].clone()).unwrap();
    assert_eq!(
        meanings.iter().map(|root| root.root).collect::<Vec<_>>(),
        vec![
            canonical::RootV43::SpellCreation,
            canonical::RootV43::ReflexiveTap
        ]
    );
    for root in &meanings {
        canonical::validate(&root.selector, root).unwrap();
        assert_eq!(root.token_card_id, token_id);
        assert_eq!(root.token_count, 2);
        assert_eq!(root.spell_target_count, 0);
        assert_eq!(root.target_count, 1);
        let mut bad = root.clone();
        bad.token_count = 1;
        assert!(canonical::validate(&root.selector, &bad).is_err());
        let mut bad = root.clone();
        bad.source_survival_required = true;
        assert!(canonical::validate(&root.selector, &bad).is_err());
    }
    let base = super::baseline(id, &recipe(id)).unwrap();
    assert_eq!(
        crate::current_mixed_policy_v1::roots::baseline(id).unwrap(),
        base
    );
    assert_eq!(
        base["definition_owned_family_meanings"],
        p["canonical_family_roots"]
    );
    assert!(validate_shape(token_id, &recipe(token_id)).unwrap());
    // Existing generic token serializer prefix remains stable. The new spell
    // supplement authenticates its complete token blueprint independently.
    let token_base = crate::current_mixed_policy_v1::roots::baseline(token_id).unwrap();
    assert_eq!(token_base["primitive_printing"]["token"], true);
    assert_eq!(
        token_base["primitive_printing"]["keywords"],
        Keywords::FLYING.0
    );
    assert!(printing(token_id, &recipe(token_id)).is_err());
    for card in [id, token_id] {
        macro_rules! reject {($field:ident,$value:expr)=>{{let mut changed=recipe(card);changed.$field=$value;
            assert!(matches!(validate_shape(card,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==card),stringify!($field));
            if card==id {assert!(matches!(super::baseline(card,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==card),stringify!($field));assert!(matches!(printing(card,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==card),stringify!($field));}
        }};}
        reject!(name, "Llanowar Elves");
        reject!(object_name, "Llanowar Elves");
        reject!(capability, CardCapability::Partial);
        reject!(
            cost,
            Cost {
                generic: 1,
                pips: &[],
                x_count: 0
            }
        );
        reject!(mana_value, 7);
        reject!(types, &[CardType::Sorcery]);
        reject!(colors, &[ManaColor::B]);
        reject!(subtypes, &[Subtype::Vampire]);
        reject!(supertypes, &[crate::card_def::Supertype::Legendary]);
        reject!(power, Some(7));
        reject!(toughness, Some(7));
        reject!(keywords, Keywords::FLASH);
        reject!(target_spec, TargetSpec::Creature);
        reject!(is_token, card == id);
        reject!(is_land, true);
        reject!(enters_battlefield_tapped, true);
        reject!(minimum_blockers, 2);
        reject!(spell_effect, || Some(EffectOp::Sequence(vec![])));
        reject!(spell_cannot_be_countered, true);
        reject!(records_mana_spent, true);
        reject!(additional_cost, Some(crate::eaten_alive_v1::ADDITIONAL));
        reject!(produces_mana, &[ManaColor::U]);
        reject!(mana_ability, || Some(EffectOp::Sequence(vec![])));
        reject!(changeling, true);
        reject!(delve, true);
        reject!(
            source_type_hexproof_v1,
            Some(crate::card_def::SourceTypeHexproofDefV1::Instants)
        );
        reject!(other_controlled_nontoken_death_v1,Some(crate::card_def::OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom));
    }
    let foreign = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
    assert!(printing(foreign, &recipe(id)).is_err());
}
