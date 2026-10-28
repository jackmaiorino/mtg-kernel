//! Exact recipe/typed meaning checks. Unit-normalization below is descriptive
//! only; genuine owning action/restore proofs live in ordinary_triggers/crystal.
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
        game_outcome_protection_v1: None,
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
        damage_replacement_v1: None,
        token_counter_replacement_v48: None,
        starting_life_self_bonuses_v1: d.starting_life_self_bonuses_v1,
        source_type_hexproof_v1: None,
        creature_type_entry_v1: None,
        other_controlled_nontoken_death_v1: None,
    }
}

#[test]
fn crystal_closed_recipe_structured_cast_and_both_canonical_statics() {
    let id = crate::card_def::card_id_by_name("Crystal Barricade").unwrap();
    let value = printing(id, &recipe(id)).unwrap().unwrap();
    assert_eq!(
        registered_permanent_printing(id).unwrap(),
        Some(value.clone())
    );
    let p = &value["registered_permanent_printing"];
    assert_eq!(
        p["characteristics"]["types"],
        json!(["Artifact", "Creature"])
    );
    assert_eq!(p["characteristics"]["subtypes"], json!(["Wall"]));
    assert_eq!(p["characteristics"]["supertypes"], json!([]));
    assert_eq!(p["characteristics"]["colors"], json!(["W"]));
    assert_eq!(p["characteristics"]["power"], 0);
    assert_eq!(p["characteristics"]["toughness"], 4);
    assert_eq!(p["characteristics"]["mana_value"], 2);
    assert_eq!(
        p["characteristics"]["ordered_keywords"],
        json!(["Defender"])
    );
    assert_eq!(p["characteristics"]["printed_activated_abilities"], 0);
    assert_eq!(p["characteristics"]["printed_mana_abilities"], 0);
    let cast = &p["ordered_cast_roots"][0];
    assert_eq!(p["ordered_cast_roots"].as_array().unwrap().len(), 1);
    assert_eq!(cast["method"], "normal");
    assert_eq!(cast["ordinary_origin"], "Hand");
    assert_eq!(cast["ordinary_route"], "Hand");
    assert_eq!(cast["timing"], "CasterMainPhaseEmptyStackWithPriority");
    assert_eq!(cast["targets"], "None");
    assert_eq!(
        cast["ordered_costs"],
        json!([{"PrintedMana":{"generic":1,"pips":[{"Colored":"W"}],"x_count":0}}])
    );
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
    let roots: Vec<canonical::MeaningV1> =
        serde_json::from_value(p["canonical_family_roots"].clone()).unwrap();
    assert_eq!(roots.len(), 2);
    assert_eq!(
        roots[0].static_root,
        canonical::StaticRootV1::ControllerHexproof
    );
    assert_eq!(
        roots[1].static_root,
        canonical::StaticRootV1::OtherControlledCreatureNoncombatPrevention
    );
    for root in &roots {
        canonical::validate(root).unwrap();
        assert!(
            root.current_controller_is_beneficiary
                && root.source_requires_current_front_face_battlefield_and_active_printed_abilities
                && root.departure_or_ability_suppression_ends_rule_immediately
                && root.control_change_updates_beneficiary_immediately
        );
        let mut forged = root.clone();
        forged.current_controller_is_beneficiary = false;
        assert!(canonical::validate(&forged).is_err());
    }
    assert!(matches!(
        roots[0].rule,
        canonical::RuleV1::ControllerHexproof {
            opponents_cannot_target_controller_with_spells_or_abilities: true,
            controller_can_target_themself: true,
            does_not_grant_object_hexproof: true,
            target_legality_rechecked_at_resolution: true
        }
    ));
    assert!(matches!(
        roots[1].rule,
        canonical::RuleV1::OtherControlledCreatureNoncombatPrevention {
            prevent_all_positive_noncombat_damage: true,
            exclude_this_source_permanent: true,
            combat_and_player_damage_unaffected: true,
            fully_prevented_damage_is_not_committed: true,
            ..
        }
    ));

    macro_rules! reject {($field:ident,$value:expr)=>{{let mut changed=recipe(id);changed.$field=$value;
        assert!(matches!(printing(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));
        assert!(matches!(super::baseline(id,&changed),Err(Error::UnsupportedPrintedRoot(actual))if actual==id),stringify!($field));}};}
    reject!(name, "Llanowar Elves");
    reject!(object_name, "Llanowar Elves");
    reject!(capability, CardCapability::Partial);
    reject!(cost, Cost::zero());
    reject!(mana_value, 3);
    reject!(types, &[CardType::Creature]);
    reject!(subtypes, &[Subtype::Human]);
    reject!(supertypes, &[crate::card_def::Supertype::Legendary]);
    reject!(colors, &[ManaColor::G]);
    reject!(power, Some(1));
    reject!(toughness, Some(3));
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
    reject!(
        other_controlled_nontoken_death_v1,
        Some(
            crate::card_def::OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom
        )
    );
    reject!(produces_mana, &[ManaColor::W]);
    let foreign = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
    assert!(printing(foreign, &recipe(id)).is_err());
}

fn field_count(tree: &crate::current_mixed_policy_v1::NumericTree, key: &str) -> usize {
    let rows = tree.rows();
    rows.iter()
        .enumerate()
        .filter(|(i, row)| {
            row[0] == 9
                && row[1] as usize == key.len()
                && key.bytes().enumerate().all(|(n, b)| {
                    rows.get(i + n + 1)
                        .is_some_and(|r| r[0] == 8 && r[1] == u16::from(b))
                })
        })
        .count()
}
#[test]
fn genuine_full_static_meaning_covered_id_still_receives_mandatory_printing() {
    let id = crate::card_def::card_id_by_name("Crystal Barricade").unwrap();
    let root = meanings(id).unwrap().remove(0);
    let known = crate::rl::CardStableRefV1 {
        arena_id: 3,
        card_db_id: id,
        owner: crate::ids::PlayerId::P0.into(),
        controller: crate::ids::PlayerId::P0.into(),
        zone: Zone::Hand,
        zone_change_count: 0,
    };
    let pass = crate::rl::ActionSemanticV1::Pass {
        actor: crate::ids::PlayerId::P0.into(),
    };
    for source in [
        crate::current_mixed_policy_v1::Source::FakeV50,
        crate::current_mixed_policy_v1::Source::RavenousV51,
        crate::current_mixed_policy_v1::Source::ElendaV52,
        crate::current_mixed_policy_v1::Source::PreparedV57,
    ] {
        // An exact actual canonical body marks `covered` here, independently
        // of the current transport, which supplies static source facts only.
        let packet = crate::current_mixed_policy_v1::normalize::encode(
            source,
            json!({"root":root,"known":known}),
            vec![serde_json::to_value(&pass).unwrap()],
        )
        .unwrap();
        assert_eq!(
            field_count(packet.state(), "registered_permanent_printing"),
            1
        );
        assert_eq!(
            field_count(packet.state(), "definition_owned_family_meanings"),
            0
        );
        assert!(
            field_count(
                packet.state(),
                "opponents_cannot_target_controller_with_spells_or_abilities"
            ) > 0
        );
        assert!(field_count(packet.state(), "prevent_all_positive_noncombat_damage") > 0);
    }
}
