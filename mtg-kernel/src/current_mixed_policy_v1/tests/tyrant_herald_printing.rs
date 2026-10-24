//! Closed recipes, exact canonical roots and structured operands. Runtime
//! ownership/payment paths live in ordinary_triggers/tyrant_herald.rs.
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
fn both_closed_printings_preserve_characteristics_cast_timing_and_canonical_statics() {
    for (name, cost, mv, subtypes, color, power, toughness, keywords, timing) in [
        (
            "Twinflame Tyrant",
            json!({"generic":3,"pips":[{"Colored":"R"},{"Colored":"R"}],"x_count":0}),
            5,
            "Dragon",
            "R",
            3,
            5,
            json!(["Flying"]),
            "CasterMainPhaseEmptyStackWithPriority",
        ),
        (
            "Herald of Eternal Dawn",
            json!({"generic":4,"pips":[{"Colored":"W"},{"Colored":"W"},{"Colored":"W"}],"x_count":0}),
            7,
            "Angel",
            "W",
            6,
            6,
            json!(["Flash", "Flying"]),
            "InstantWithPriorityIncludingOpponentsTurnAndNonemptyStack",
        ),
    ] {
        let id = crate::card_def::card_id_by_name(name).unwrap();
        let actual = printing(id, &recipe(id)).unwrap().unwrap();
        assert_eq!(
            registered_permanent_printing(id).unwrap(),
            Some(actual.clone())
        );
        let p = &actual["registered_permanent_printing"];
        let c = &p["characteristics"];
        assert_eq!(c["types"], json!(["Creature"]));
        assert_eq!(c["subtypes"], json!([subtypes]));
        assert_eq!(c["supertypes"], json!([]));
        assert_eq!(c["colors"], json!([color]));
        assert_eq!(c["mana_value"], mv);
        assert_eq!(c["power"], power);
        assert_eq!(c["toughness"], toughness);
        assert_eq!(c["ordered_keywords"], keywords);
        assert_eq!(c["printed_activated_abilities"], 0);
        assert_eq!(c["printed_mana_abilities"], 0);
        assert_eq!(c["minimum_blockers"], 1);
        for key in ["token", "land", "enters_tapped"] {
            assert_eq!(c[key], false);
        }
        assert_eq!(p["ordered_cast_roots"].as_array().unwrap().len(), 1);
        let cast = &p["ordered_cast_roots"][0];
        assert_eq!(cast["method"], "normal");
        assert_eq!(cast["ordinary_origin"], "Hand");
        assert_eq!(cast["ordinary_route"], "Hand");
        assert_eq!(cast["timing"], timing);
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
        assert_eq!(p["canonical_family_roots"].as_array().unwrap().len(), 1);
        if name == "Twinflame Tyrant" {
            let root: crate::twinflame_tyrant_v1::PrintedMeaningV1 =
                serde_json::from_value(p["canonical_family_roots"][0].clone()).unwrap();
            root.validate().unwrap();
            assert!(
                root.current_source_controller
                    && root.damage_source_captured_controller_when_historical
                    && root.opponent_player_or_opponent_controlled_permanent
                    && root.combat_and_noncombat
                    && root.each_live_source_once_per_event
                    && root.affected_controller_orders_noncommuting_effects
            );
            let mut forged = root.clone();
            forged.current_source_controller = false;
            assert!(forged.validate().is_err());
            assert_eq!(
                p["ordered_damage_replacements"].as_array().unwrap().len(),
                1
            );
            let r = &p["ordered_damage_replacements"][0];
            assert_eq!(
                r["ordered_damage_changes"],
                json!([{"MultiplyPositiveDamage":{"factor":2}}])
            );
            assert_eq!(
                r["damage_controller"],
                "CurrentDamageSourceControllerOrOriginalCapturedControllerForHistoricalDamage"
            );
            assert_eq!(
                r["recipient"],
                "OpposingPlayerOrOpposingControlledBattlefieldPermanent"
            );
            assert_eq!(r["source_zone"], "Battlefield");
            for key in [
                "requires_current_front_face_and_active_printed_ability",
                "current_replacement_source_controller_must_equal_damage_controller",
                "combat_and_noncombat",
                "each_live_source_once_per_event",
                "affected_controller_orders_noncommuting_effects",
                "checked_amount_before_damage_mutation",
            ] {
                assert_eq!(r[key], true);
            }
        } else {
            use crate::rules_meaning_game_outcome_protection_v1 as canonical;
            let root: canonical::MeaningV1 =
                serde_json::from_value(p["canonical_family_roots"][0].clone()).unwrap();
            canonical::validate(&root).unwrap();
            assert!(
                root.flash
                    && root.flying
                    && root.controller_cannot_lose_game
                    && root.opponents_cannot_win_game
                    && root.current_controller_and_active_battlefield_printed_ability_required
                    && root.applies_to_life_poison_and_empty_draw_losses
                    && root.protection_sampled_before_simultaneous_sba_departures
                    && root.protected_empty_draw_attempt_expires_at_its_sba_check
                    && root.life_and_poison_conditions_persist_after_source_leaves
            );
            let mut forged = root.clone();
            forged.flash = false;
            assert!(canonical::validate(&forged).is_err());
            assert_eq!(p["ordered_damage_replacements"], json!([]));
        }
        macro_rules! reject {($field:ident,$value:expr)=>{{let mut changed=recipe(id);changed.$field=$value;
            assert!(matches!(printing(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));
            assert!(matches!(super::baseline(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));}};}
        reject!(name, "Llanowar Elves");
        reject!(object_name, "Llanowar Elves");
        reject!(capability, CardCapability::Partial);
        reject!(cost, Cost::zero());
        reject!(mana_value, 0);
        reject!(types, &[CardType::Enchantment]);
        reject!(subtypes, &[Subtype::Wall]);
        reject!(supertypes, &[crate::card_def::Supertype::Legendary]);
        reject!(colors, &[ManaColor::G]);
        reject!(power, Some(0));
        reject!(toughness, Some(1));
        reject!(keywords, Keywords::NONE);
        reject!(minimum_blockers, 2);
        reject!(target_spec, TargetSpec::AnyPlayer);
        reject!(spell_effect, || None);
        reject!(spell_cannot_be_countered, true);
        reject!(records_mana_spent, true);
        reject!(is_token, true);
        reject!(is_land, true);
        reject!(enters_battlefield_tapped, true);
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
            activated_abilities,
            CARD_DEFS[usize::from(crate::card_def::card_id_by_name("Sower of Chaos").unwrap())]
                .activated_abilities
        );
        reject!(
            controlled_creature_grant,
            Some(crate::controlled_creature_grants_v1::MEMORIAL)
        );
        reject!(
            source_type_hexproof_v1,
            Some(crate::card_def::SourceTypeHexproofDefV1::Instants)
        );
        reject!(other_controlled_nontoken_death_v1,Some(crate::card_def::OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom));
        reject!(produces_mana, &[ManaColor::W]);
        reject!(
            damage_replacement_v1,
            if name == "Twinflame Tyrant" {
                None
            } else {
                Some(DamageReplacementDefV1::DoubleControlledSourceToOpponent)
            }
        );
        reject!(
            game_outcome_protection_v1,
            if name == "Herald of Eternal Dawn" {
                None
            } else {
                Some(GameOutcomeProtectionDefV1::ControllerCannotLoseAndOpponentsCannotWin)
            }
        );
        let foreign = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        assert!(printing(foreign, &recipe(id)).is_err());
    }
}
