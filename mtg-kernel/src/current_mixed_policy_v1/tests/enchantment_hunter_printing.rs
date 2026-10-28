//! Closed printing descriptors and canonical root authentication only.
//! Genuine owning casts, triggers and private observations live in ordinary_triggers.
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
        creature_type_entry_v1: None,
        other_controlled_nontoken_death_v1: None,
    }
}

#[test]
fn three_closed_printings_preserve_ordinary_cast_and_all_distinct_canonical_roots() {
    for (name, color, generic, mv, hunter) in [
        ("Banishing Light", "W", 2, 3, false),
        ("Garruk's Uprising", "G", 2, 3, false),
        ("High-Society Hunter", "B", 3, 5, true),
    ] {
        let id = crate::card_def::card_id_by_name(name).unwrap();
        let value = printing(id, &recipe(id)).unwrap().unwrap();
        assert_eq!(
            registered_permanent_printing(id).unwrap(),
            Some(value.clone())
        );
        let p = &value["registered_permanent_printing"];
        let c = &p["characteristics"];
        assert_eq!(
            c["types"],
            if hunter {
                json!(["Creature"])
            } else {
                json!(["Enchantment"])
            }
        );
        assert_eq!(
            c["subtypes"],
            if hunter {
                json!(["Vampire", "Noble"])
            } else {
                json!([])
            }
        );
        assert_eq!(c["supertypes"], json!([]));
        assert_eq!(c["colors"], json!([color]));
        assert_eq!(c["mana_value"], mv);
        assert_eq!(c["power"], if hunter { json!(5) } else { Value::Null });
        assert_eq!(c["toughness"], if hunter { json!(3) } else { Value::Null });
        assert_eq!(
            c["ordered_keywords"],
            if hunter { json!(["Flying"]) } else { json!([]) }
        );
        assert_eq!(c["minimum_blockers"], 1);
        for key in ["token", "land", "enters_tapped"] {
            assert_eq!(c[key], false);
        }
        for key in ["printed_mana_abilities", "printed_activated_abilities"] {
            assert_eq!(c[key], 0);
        }
        let cost = json!({"generic":generic,"pips":if hunter {vec![json!({"Colored":color});2]} else {vec![json!({"Colored":color})]},"x_count":0});
        let cast = &p["ordered_cast_roots"][0];
        assert_eq!(p["ordered_cast_roots"].as_array().unwrap().len(), 1);
        assert_eq!(cast["method"], "normal");
        assert_eq!(cast["ordinary_route"], "Hand");
        assert_eq!(cast["ordinary_origin"], "Hand");
        assert_eq!(cast["timing"], "CasterMainPhaseEmptyStackWithPriority");
        assert_eq!(cast["targets"], "None");
        assert_eq!(cast["ordered_costs"], json!([{"PrintedMana":cost}]));
        assert_eq!(
            cast["ordered_resolution"],
            json!(["MoveOriginalSpellFromStackToBattlefieldUnderCasterControl"])
        );
        assert_eq!(
            cast["physical_departure"],
            "BattlefieldOnResolutionOtherwiseGraveyard"
        );
        assert_eq!(
            cast["other_cast_permissions_require_current_candidate_authority"],
            true
        );
        assert_eq!(cast["zone_replacements_still_apply"], true);
        let baseline = super::baseline(id, &recipe(id)).unwrap();
        assert_eq!(
            crate::current_mixed_policy_v1::roots::baseline(id).unwrap(),
            baseline
        );
        assert_eq!(
            baseline["definition_owned_family_meanings"],
            p["canonical_family_roots"]
        );
        let values = p["canonical_family_roots"].as_array().unwrap();
        assert_eq!(values.len(), 3);
        let mut hashes = Vec::new();
        for value in values {
            hashes.push(value["root_sha256"].as_str().unwrap().to_owned());
            match name {
                "Banishing Light" => {
                    use crate::rules_meaning_banishing_v1 as m;
                    let root: m::CardMeaningV1 = serde_json::from_value(value.clone()).unwrap();
                    m::validate_meaning_v1(&root).unwrap();
                    let mut bad = root;
                    bad.facts.clear();
                    assert!(m::validate_meaning_v1(&bad).is_err());
                }
                "Garruk's Uprising" => {
                    use crate::rules_meaning_garruk_v1 as m;
                    let root: m::CardMeaningV1 = serde_json::from_value(value.clone()).unwrap();
                    m::validate_meaning_v1(&root).unwrap();
                    let mut bad = root;
                    bad.facts.clear();
                    assert!(m::validate_meaning_v1(&bad).is_err());
                }
                _ => {
                    use crate::rules_meaning_high_society_hunter_v1 as m;
                    let root: m::MeaningV1 = serde_json::from_value(value.clone()).unwrap();
                    m::validate(&root).unwrap();
                    let mut bad = root;
                    bad.ordered_facts.clear();
                    assert!(m::validate(&bad).is_err());
                }
            }
        }
        hashes.sort();
        hashes.dedup();
        assert_eq!(hashes.len(), 3);
        macro_rules! reject {($field:ident,$value:expr)=>{{let mut changed=recipe(id);changed.$field=$value;
            assert!(matches!(printing(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));
            assert!(matches!(super::baseline(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));}};}
        reject!(name, "Llanowar Elves");
        reject!(object_name, "Llanowar Elves");
        reject!(capability, CardCapability::Partial);
        reject!(cost, Cost::zero());
        reject!(mana_value, 0);
        reject!(types, &[CardType::Artifact]);
        reject!(subtypes, &[Subtype::Wall]);
        reject!(supertypes, &[crate::card_def::Supertype::Legendary]);
        reject!(colors, &[ManaColor::U]);
        reject!(power, Some(0));
        reject!(toughness, Some(1));
        reject!(keywords, Keywords::TRAMPLE);
        reject!(minimum_blockers, 2);
        reject!(target_spec, TargetSpec::AnyPlayer);
        reject!(spell_effect, || None);
        reject!(mana_ability, || Some(EffectOp::Sequence(vec![])));
        reject!(spell_cannot_be_countered, true);
        reject!(records_mana_spent, true);
        reject!(is_token, true);
        reject!(is_land, true);
        reject!(enters_battlefield_tapped, true);
        reject!(delve, true);
        reject!(changeling, true);
        reject!(produces_mana, &[ManaColor::W]);
        reject!(
            source_type_hexproof_v1,
            Some(crate::card_def::SourceTypeHexproofDefV1::Instants)
        );
        reject!(other_controlled_nontoken_death_v1,Some(crate::card_def::OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom));
        reject!(
            controlled_creature_grant,
            Some(crate::controlled_creature_grants_v1::MEMORIAL)
        );
        reject!(
            damage_replacement_v1,
            Some(crate::card_def::DamageReplacementDefV1::DoubleControlledSourceToOpponent)
        );
        reject!(game_outcome_protection_v1,Some(crate::card_def::GameOutcomeProtectionDefV1::ControllerCannotLoseAndOpponentsCannotWin));
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
            activated_abilities,
            CARD_DEFS[usize::from(crate::card_def::card_id_by_name("Sower of Chaos").unwrap())]
                .activated_abilities
        );
        let foreign = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        assert!(printing(foreign, &recipe(id)).is_err());
    }
}
