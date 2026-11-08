//! Finite exact CardDef/program extraction for the three issue-454 mainboards.
//! This is printed data only. External static handlers and public dynamic
//! operands require separate binding before this can admit an actor packet.
use crate::card_def::*;
use crate::rl::RlContractError;
use serde_json::{json, Value};
type Result<T> = std::result::Result<T, RlContractError>;

pub const REPORTED_MAINBOARD_NAMES: &[&str] = &[
    "Ancient Tomb",
    "Animate Dead",
    "Archon of Cruelty",
    "Atraxa, Grand Unifier",
    "Bitter Triumph",
    "Black Lotus",
    "Bloodstained Mire",
    "Brainstorm",
    "Bridgeworks Battle",
    "Carnivorous Cultivator",
    "Cavern of Souls",
    "Chalice of the Void",
    "Collector Ouphe",
    "Consider",
    "Dark Matter Manipulator",
    "Dark Ritual",
    "Daze",
    "Doomsday",
    "Duress",
    "Edge of Autumn",
    "Eldrazi Temple",
    "Elvish Spirit Guide",
    "Flooded Strand",
    "Flow State",
    "Force of Will",
    "Forest",
    "Island",
    "Jace, Wielder of Mysteries",
    "Lion's Eye Diamond",
    "Lotus Petal",
    "Mana Crypt",
    "Misdirection",
    "Misty Rainforest",
    "Mox Emerald",
    "Null Rod",
    "Once Upon a Time",
    "Personal Tutor",
    "Pithing Needle",
    "Polluted Delta",
    "Ponder",
    "Reanimate",
    "Scalding Tarn",
    "Show and Tell",
    "Snow-Covered Island",
    "Snow-Covered Swamp",
    "Sol Ring",
    "Sowing Mycospawn",
    "Street Wraith",
    "Strip Mine",
    "Swamp",
    "Thassa's Oracle",
    "Thief of Existence",
    "Thorn of Amethyst",
    "Thoughtseize",
    "Trinisphere",
    "Undercity Sewers",
    "Underground Sea",
    "Urza's Saga",
    "Vexing Bauble",
    "Wasteland",
    "Wastescape Battlemage",
    "Yavimaya, Cradle of Growth",
];
pub const GENERATED_PRINTING_NAMES: &[&str] = &["Karn Construct Token", "Enroot"];

fn finite_printing(name: &str) -> bool {
    REPORTED_MAINBOARD_NAMES.contains(&name) || GENERATED_PRINTING_NAMES.contains(&name)
}

fn unsupported() -> RlContractError {
    RlContractError("unrepresented Eternal printed definition field".into())
}

pub fn printing_v1(id: u16) -> Result<Value> {
    let d = CARD_DEFS.get(usize::from(id)).ok_or_else(unsupported)?;
    if !finite_printing(d.name) {
        return Err(RlContractError(
            "printing outside finite Eternal descriptor pool".into(),
        ));
    }
    // Exhaustive destructuring makes a new CardDef field a compile error here.
    let CardDef {
        name: _,
        activated_abilities,
        activated_ability_generic_reductions,
        additional_cost,
        additional_mana_abilities,
        additional_mana_ability_conditions,
        additional_modes,
        adventure,
        alt_cost,
        animation,
        as_enters_choose_color_other_than,
        as_enters_color_choice_v1,
        attachment,
        bestow,
        cant_be_blocked_by_monarchs_creatures,
        capability,
        changeling,
        chosen_color_creature_boost_v1,
        colors,
        combat_static_rules,
        conditional_tap_yield,
        controlled_counter_keyword,
        controlled_creature_grant,
        cost,
        creature_type_entry_v1,
        damage_replacement_v1,
        delve,
        enters_battlefield_tapped,
        enters_battlefield_tapped_unless,
        enters_tapped_unless_controller,
        enters_with_plus_one_counters,
        equipment,
        escape,
        flashback,
        game_outcome_protection_v1,
        generic_cost_reduction,
        is_land,
        is_token,
        keywords,
        kicked_target_spec,
        kicker_cost,
        madness_cost,
        mana_ability,
        mana_ability_choices,
        mana_ability_def,
        mana_ability_includes_chosen_color,
        mana_value,
        minimum_blockers,
        mode2,
        mode3,
        object_name,
        omen,
        optional_additional_cost,
        other_controlled_nontoken_death_v1,
        plot_cost,
        power,
        produces_mana,
        records_mana_spent,
        restricted_mana_abilities,
        saga,
        source_type_hexproof_v1,
        spell_cannot_be_countered,
        spell_effect,
        starting_life_self_bonuses_v1,
        starting_loyalty,
        subtypes,
        supertypes,
        target_spec,
        token_counter_replacement_v48,
        toughness,
        transform_face,
        types,
        ward_cost,
    } = d;
    if !activated_ability_generic_reductions.is_empty()
        || !additional_mana_abilities.is_empty()
        || !additional_mana_ability_conditions.is_empty()
        || !additional_modes.is_empty()
        || adventure.is_some()
        || animation.is_some()
        || as_enters_choose_color_other_than.is_some()
        || as_enters_color_choice_v1.is_some()
        || bestow.is_some()
        || *cant_be_blocked_by_monarchs_creatures
        || *capability != CardCapability::Full
        || *changeling
        || chosen_color_creature_boost_v1.is_some()
        || combat_static_rules.must_attack_each_combat
        || combat_static_rules.cant_be_blocked_by_subtype.is_some()
        || controlled_counter_keyword.is_some()
        || controlled_creature_grant.is_some()
        || creature_type_entry_v1.is_some()
        || damage_replacement_v1.is_some()
        || *delve
        || enters_battlefield_tapped_unless.is_some()
        || enters_tapped_unless_controller.is_some()
        || enters_with_plus_one_counters.is_some()
        || equipment.is_some()
        || escape.is_some()
        || flashback.is_some()
        || game_outcome_protection_v1.is_some()
        || generic_cost_reduction.is_some()
        || kicked_target_spec.is_some()
        || madness_cost.is_some()
        || *mana_ability_includes_chosen_color
        || *minimum_blockers != 1
        || mode2.is_some()
        || mode3.is_some()
        || omen.is_some()
        || optional_additional_cost.is_some()
        || other_controlled_nontoken_death_v1.is_some()
        || plot_cost.is_some()
        || *records_mana_spent
        || source_type_hexproof_v1.is_some()
        || *spell_cannot_be_countered
        || !starting_life_self_bonuses_v1.is_empty()
        || token_counter_replacement_v48.is_some()
        || ward_cost.is_some()
    {
        return Err(unsupported());
    }
    let mut out = serde_json::Map::new();
    macro_rules! field {
        ($key:literal, $value:expr) => {
            out.insert($key.into(), json!($value));
        };
    }
    field!("schema", "eternal-printed-programs/v1");
    field!("external_rules", external_rules(d.name)?);
    field!("cost", cost);
    field!("types", types);
    field!("subtypes", subtypes);
    field!(
        "supertypes",
        supertypes
            .iter()
            .map(|x| match x {
                Supertype::Basic => "Basic",
                Supertype::Snow => "Snow",
                Supertype::Legendary => "Legendary",
            })
            .collect::<Vec<_>>()
    );
    field!("colors", colors);
    field!("power", power);
    field!("toughness", toughness);
    field!("starting_loyalty", starting_loyalty);
    field!("is_land", is_land);
    field!("is_token", is_token);
    field!("produces_mana", produces_mana);
    field!("target_spec", target_spec);
    field!("keywords", keywords);
    field!("spell_effect", spell_effect());
    field!("mana_ability", mana_ability());
    field!("kicker_cost", kicker_cost);
    field!("mana_ability_choices", mana_ability_choices);
    field!("enters_battlefield_tapped", enters_battlefield_tapped);
    field!("mana_value", mana_value);
    field!("conditional_tap_yield", conditional_tap_yield);
    field!("alt_cost", alt_cost.as_ref().map(alternative).transpose()?);
    field!("additional_cost", additional_cost.map(costs).transpose()?);
    field!(
        "activated_abilities",
        activated_abilities
            .iter()
            .map(activated)
            .collect::<Result<Vec<_>>>()?
    );
    field!(
        "mana_ability_def",
        mana_ability_def.as_ref().map(mana).transpose()?
    );
    field!(
        "restricted_mana_abilities",
        restricted_mana_abilities
            .iter()
            .map(|a| json!({"colors": a.colors, "restriction": a.restriction}))
            .collect::<Vec<_>>()
    );
    field!(
        "attachment",
        attachment.as_ref().map(attachment_value).transpose()?
    );
    field!("transform_face", transform_face.as_ref().map(|f| json!({"types": f.types, "subtypes": f.subtypes, "colors": f.colors, "power": f.power, "toughness": f.toughness, "keywords": f.keywords})));
    field!(
        "saga",
        saga.as_ref()
            .map(|s| s.chapter_effects.iter().map(|f| f()).collect::<Vec<_>>())
    );
    field!(
        "triggers",
        crate::trigger::triggers_for(id)
            .iter()
            .map(trigger)
            .collect::<Result<Vec<_>>>()?
    );
    // Printing identity is bound by the caller; display text is not meaning.
    let _ = object_name;
    Ok(Value::Object(out))
}

fn costs(items: &[CostComponent]) -> Result<Vec<Value>> {
    items
        .iter()
        .map(|c| {
            Ok(match c {
                CostComponent::Tap => json!({"Tap": true}),
                CostComponent::SacrificeSelf => json!({"SacrificeSelf": true}),
                CostComponent::ExileSelf => json!({"ExileSelf": true}),
                CostComponent::DiscardSelf => json!({"DiscardSelf": true}),
                CostComponent::DiscardCards(n) => json!({"DiscardCards": n}),
                CostComponent::Mana(m) => json!({"Mana": m}),
                CostComponent::PayLife(n) => json!({"PayLife": n}),
                CostComponent::Loyalty(n) => json!({"Loyalty": n}),
                CostComponent::ExileCardFromHandWithColorV1(c) => {
                    json!({"ExileCardFromHandWithColor": c})
                }
                CostComponent::ReturnControlledPermanentToOwnersHand(
                    PermanentFilterDef::LandWithSubtype(s),
                ) => json!({"ReturnControlledLandWithSubtypeToOwnersHand": s}),
                CostComponent::SacrificeControlled {
                    count,
                    filter: PermanentFilter::Land,
                } => json!({"SacrificeControlledLands": count}),
                _ => return Err(unsupported()),
            })
        })
        .collect()
}

/// Explicit rules implemented outside the generated definition programs.
/// Static-source rules use the owning hook's live battlefield/printed-ability
/// predicate. This describes native behavior, including its documented limits.
#[derive(serde::Serialize)]
enum ExternalRule {
    ChooseCreatureSubtypeOnEntry,
    RestrictedChosenTypeCreatureManaMakesCastUncounterable,
    ProhibitAllBattlefieldArtifactActivationsIncludingMana,
    SelfPowerPerControllerGraveyardGroup {
        cards_per_group: u8,
        power: i16,
    },
    AutomaticColorlessEldraziSpellTapYield {
        colorless: u8,
        surplus_retains_restriction: bool,
    },
    ReplaceControllerEmptyLibraryDrawWithWin,
    ManaActivationRequiresPriorityWithoutPendingCastActivationEffectOrDiscard,
    AlternativeCastRequiresNoPreviousSpellCastThisGame,
    ChooseVisibleCardNameOnEntry,
    ProhibitNonmanaActivationsOfChosenEffectiveName,
    NoncreatureSpellGenericTaxPerActiveSource {
        generic: u8,
    },
    UntappedSourceRaisesFinalSpellManaCostTo {
        minimum: u8,
    },
    SagaGrantedAbilityRequiresResolvedChapter {
        chapter: u8,
        ability: SagaAbility,
    },
    FaceUpPrintedLandsGainForest {
        add_green_tap_unless_already_producing_green: bool,
    },
    SelfPowerToughnessPerControlledArtifact {
        power: i16,
        toughness: i16,
    },
    PreparedCopyCastPermissionFollowsPreparedCreatureControllerUntilCastOrCreatureDeparture,
    AnimateDeadGraveyardAttachmentThenReanimatedCreatureAttachment,
    LandBackFacePlayableFromHand {
        face: u8,
        untapped_life_payment: u8,
    },
    ChargeCountersOnEntryEqualPaidX,
    NormalCastRequiresAdditionalDiscardAlternativeDoesNot,
}
#[derive(serde::Serialize)]
enum SagaAbility {
    PrimaryColorlessMana,
    ActivatedConstruct,
}

fn external_rules(name: &str) -> Result<Vec<ExternalRule>> {
    use ExternalRule::*;
    if !finite_printing(name) {
        return Err(unsupported());
    }
    Ok(match name {
        "Cavern of Souls" => vec![
            ChooseCreatureSubtypeOnEntry,
            RestrictedChosenTypeCreatureManaMakesCastUncounterable,
        ],
        "Collector Ouphe" | "Null Rod" => {
            vec![ProhibitAllBattlefieldArtifactActivationsIncludingMana]
        }
        "Dark Matter Manipulator" => vec![SelfPowerPerControllerGraveyardGroup {
            cards_per_group: 7,
            power: 2,
        }],
        "Eldrazi Temple" => vec![AutomaticColorlessEldraziSpellTapYield {
            colorless: 2,
            surplus_retains_restriction: true,
        }],
        "Jace, Wielder of Mysteries" => vec![ReplaceControllerEmptyLibraryDrawWithWin],
        "Lion's Eye Diamond" => {
            vec![ManaActivationRequiresPriorityWithoutPendingCastActivationEffectOrDiscard]
        }
        "Once Upon a Time" => vec![AlternativeCastRequiresNoPreviousSpellCastThisGame],
        "Pithing Needle" => vec![
            ChooseVisibleCardNameOnEntry,
            ProhibitNonmanaActivationsOfChosenEffectiveName,
        ],
        "Thorn of Amethyst" => vec![NoncreatureSpellGenericTaxPerActiveSource { generic: 1 }],
        "Trinisphere" => vec![UntappedSourceRaisesFinalSpellManaCostTo { minimum: 3 }],
        "Urza's Saga" => vec![
            SagaGrantedAbilityRequiresResolvedChapter {
                chapter: 1,
                ability: SagaAbility::PrimaryColorlessMana,
            },
            SagaGrantedAbilityRequiresResolvedChapter {
                chapter: 2,
                ability: SagaAbility::ActivatedConstruct,
            },
        ],
        "Yavimaya, Cradle of Growth" => {
            vec![FaceUpPrintedLandsGainForest {
                add_green_tap_unless_already_producing_green: true,
            }]
        }
        "Karn Construct Token" => vec![SelfPowerToughnessPerControlledArtifact {
            power: 1,
            toughness: 1,
        }],
        "Enroot" => vec![
            PreparedCopyCastPermissionFollowsPreparedCreatureControllerUntilCastOrCreatureDeparture,
        ],
        "Animate Dead" => vec![AnimateDeadGraveyardAttachmentThenReanimatedCreatureAttachment],
        "Bridgeworks Battle" => vec![LandBackFacePlayableFromHand {
            face: 1,
            untapped_life_payment: 3,
        }],
        "Chalice of the Void" => vec![ChargeCountersOnEntryEqualPaidX],
        "Bitter Triumph" => vec![NormalCastRequiresAdditionalDiscardAlternativeDoesNot],
        _ => Vec::new(),
    })
}

fn alternative(a: &AltCostDef) -> Result<Value> {
    let AltCostDef {
        components,
        condition,
    } = a;
    let condition = match condition {
        AltCostCondition::Always => "Always",
        AltCostCondition::FirstSpellThisGameV1 => "FirstSpellThisGame",
        _ => return Err(unsupported()),
    };
    Ok(json!({"ordered_costs": costs(components)?, "condition": condition}))
}

fn activated(a: &ActivatedAbilityDef) -> Result<Value> {
    let ActivatedAbilityDef {
        cost,
        target_spec,
        effect,
        activation_zone,
        sorcery_speed_only,
        activation_target_filter,
        max_activations_per_turn,
        face,
    } = a;
    let target_filter = match activation_target_filter {
        ActivationTargetFilter::TargetSpecOnly => json!("TargetSpecOnly"),
        ActivationTargetFilter::CreatureBlockedBySource => json!("CreatureBlockedBySource"),
        ActivationTargetFilter::CreatureSubtype(s) => json!({"CreatureSubtype": s}),
    };
    Ok(
        json!({"ordered_costs": costs(cost)?, "target_spec": target_spec,
        "program": effect(), "activation_zone": activation_zone,
        "sorcery_speed_only": sorcery_speed_only, "target_filter": target_filter,
        "max_activations_per_turn": max_activations_per_turn, "face": face}),
    )
}

fn mana(a: &ManaAbilityDef) -> Result<Value> {
    let ManaAbilityDef {
        cost,
        amount,
        controller_damage,
        max_activations_per_turn,
    } = a;
    let cost = match cost {
        ManaAbilityCostDef::TapSelf => "TapSelf",
        ManaAbilityCostDef::SacrificeSelf => "SacrificeSelf",
        ManaAbilityCostDef::TapAndSacrificeSelf => "TapAndSacrificeSelf",
        ManaAbilityCostDef::DiscardHandAndSacrificeSelfV1 => {
            "DiscardHandAndSacrificeSelfWithPriorityOnly"
        }
        _ => return Err(unsupported()),
    };
    let ManaAbilityAmountDef::Fixed(amount) = amount else {
        return Err(unsupported());
    };
    Ok(
        json!({"cost": cost, "amount": amount, "controller_damage": controller_damage,
        "max_activations_per_turn": max_activations_per_turn}),
    )
}

fn attachment_value(a: &AttachmentDef) -> Result<Value> {
    let AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef {
        power,
        toughness,
        keywords,
        per_controlled_subtype,
    }) = a
    else {
        return Err(unsupported());
    };
    Ok(
        json!({"AuraCreatureStatic": {"power": power, "toughness": toughness,
        "keywords": keywords, "per_controlled_subtype": per_controlled_subtype}}),
    )
}

fn trigger(t: &crate::trigger::TriggeredAbilityDef) -> Result<Value> {
    let crate::trigger::TriggeredAbilityDef {
        condition,
        home_zone,
        intervening_if_kicked,
        intervening_if_controls_another_source_card,
        face_index,
        effect,
    } = t;
    Ok(
        json!({"condition": trigger_condition(*condition)?, "home_zone": home_zone,
        "intervening_if_kicked": intervening_if_kicked,
        "intervening_if_controls_another_source_card": intervening_if_controls_another_source_card,
        "face_index": face_index, "program": effect()}),
    )
}

fn trigger_condition(t: crate::trigger::TriggerCondition) -> Result<Value> {
    use crate::trigger::TriggerCondition as T;
    Ok(match t {
        T::Etb => json!("Etb"),
        T::Attacks => json!("Attacks"),
        T::CastSpell => json!("CastSpell"),
        T::CastSelf => json!("CastSelf"),
        T::LeftBattlefield => json!("LeftBattlefield"),
        T::LeftBattlefieldToGraveyard => json!("LeftBattlefieldToGraveyard"),
        T::BeginningOfUpkeep { controller_only } => {
            json!({"BeginningOfUpkeep": {"controller_only": controller_only}})
        }
        T::StandardV1(crate::standard_cards_v1::StandardTriggerV1::EternalV1(t)) => {
            eternal_trigger(t)?
        }
        _ => return Err(unsupported()),
    })
}

fn eternal_trigger(t: crate::eternal_cards_v1::EternalTriggerV1) -> Result<Value> {
    use crate::eternal_cards_v1::EternalTriggerV1 as E;
    Ok(match t {
        E::InstantOrSorceryCardLeavesYourGraveyard => {
            json!("InstantOrSorceryCardLeavesYourGraveyard")
        }
        E::PermanentCardsToYourGraveyardWhileMinusCounter => {
            json!("PermanentCardsToYourGraveyardWhileMinusCounter")
        }
        E::OpponentDrawsExceptFirstInDrawStep => json!("OpponentDrawsExceptFirstInDrawStep"),
        E::EntersAfterCastV1 => json!("EntersAfterCast"),
        E::PutIntoGraveyardFromAnywhereV1 => json!("PutIntoGraveyardFromAnywhere"),
        E::EntersIfTombNotCompletedV1 => json!("EntersIfTombNotCompleted"),
        E::VintageV1(v) => json!({"VintageV1": match v {
            crate::eternal_vintage_v1::VintageTriggerV1::SpellCastWithoutManaV1 => "SpellCastWithoutManaV1",
            crate::eternal_vintage_v1::VintageTriggerV1::SpellCastWithoutColoredManaV1 => "SpellCastWithoutColoredManaV1",
            crate::eternal_vintage_v1::VintageTriggerV1::OpponentUpkeepV1 => "OpponentUpkeepV1",
            crate::eternal_vintage_v1::VintageTriggerV1::EndStepBeforeYourTurnWhileTappedV1 => "EndStepBeforeYourTurnWhileTappedV1",
            crate::eternal_vintage_v1::VintageTriggerV1::ControlledArtifactEntersV1 => "ControlledArtifactEntersV1",
        }}),
        E::LurrusV1(v) => json!({"LurrusV1": match v {
            crate::eternal_lurrus_v1::LurrusTriggerV1::BeginningOfEachEndStepV1 => "BeginningOfEachEndStepV1",
        }}),
        E::E5aV1(v) => json!({"E5aV1": match v {
            crate::eternal_e5a_v1::E5aTriggerV1::SpellCastManaValueEqualsChargeCountersV1 => "SpellCastManaValueEqualsChargeCountersV1",
            crate::eternal_e5a_v1::E5aTriggerV1::ControllerCastsColorlessSpellV1 => "ControllerCastsColorlessSpellV1",
            crate::eternal_e5a_v1::E5aTriggerV1::OtherColorlessControlledCreatureEntersV1 => "OtherColorlessControlledCreatureEntersV1",
            crate::eternal_e5a_v1::E5aTriggerV1::ControllerCombatIfCastNoncreatureSpellV1 => "ControllerCombatIfCastNoncreatureSpellV1",
            crate::eternal_e5a_v1::E5aTriggerV1::ControllerUpkeepIfThreeArtifactsV1 => "ControllerUpkeepIfThreeArtifactsV1",
            crate::eternal_e5a_v1::E5aTriggerV1::ControllerCastsFourthNoncreatureSpellV1 => "ControllerCastsFourthNoncreatureSpellV1",
        }}),
        E::E5bV1(v) => json!({"E5bV1": match v {
            crate::eternal_legacy_e5b_v1::E5bTriggerV1::OtherCatsYouControlDieV1 => "OtherCatsYouControlDieV1",
            crate::eternal_legacy_e5b_v1::E5bTriggerV1::YourEndStepIfGainedLifeV1 => "YourEndStepIfGainedLifeV1",
            crate::eternal_legacy_e5b_v1::E5bTriggerV1::EntersIfOpponentHasMoreLandsV1 => "EntersIfOpponentHasMoreLandsV1",
            crate::eternal_legacy_e5b_v1::E5bTriggerV1::CastSelfIfKickedV1 => "CastSelfIfKickedV1",
            crate::eternal_legacy_e5b_v1::E5bTriggerV1::CastSelfWithAlternativeCostV1 => "CastSelfWithAlternativeCostV1",
        }}),
        E::E7aV1(v) => json!({"E7aV1": match v {
            crate::eternal_legacy_e7a_v1::E7aTriggerV1::ControllerCastsArtifactSpellV1 => "ControllerCastsArtifactSpellV1",
            crate::eternal_legacy_e7a_v1::E7aTriggerV1::EntersFromYourGraveyardV1 => "EntersFromYourGraveyardV1",
            crate::eternal_legacy_e7a_v1::E7aTriggerV1::OpponentUpkeepV1 => "OpponentUpkeepV1",
        }}),
        E::E7bV1(v) => json!({"E7bV1": match v {
            crate::eternal_vintage_e7b_v1::E7bTriggerV1::PutIntoGraveyardFromLibraryV1 => "PutIntoGraveyardFromLibraryV1",
            crate::eternal_vintage_e7b_v1::E7bTriggerV1::EvokedEntersV1 => "EvokedEntersV1",
            crate::eternal_vintage_e7b_v1::E7bTriggerV1::EachEndStepV1 => "EachEndStepV1",
            crate::eternal_vintage_e7b_v1::E7bTriggerV1::YourEndStepIfGainedThreeLifeV1 => "YourEndStepIfGainedThreeLifeV1",
        }}),
        E::E8aV1(v) => json!({"E8aV1": match v {
            crate::eternal_legacy_e8a_v1::E8aTriggerV1::EntersUntappedV1 => "EntersUntappedV1",
            crate::eternal_legacy_e8a_v1::E8aTriggerV1::PlusOneCountersPutOnSelfV1 => "PlusOneCountersPutOnSelfV1",
        }}),
        _ => return Err(unsupported()),
    })
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;

    #[test]
    fn issue454_reported_mainboards_have_exact_printed_program_operands() {
        assert_eq!(REPORTED_MAINBOARD_NAMES.len(), 62);
        for name in REPORTED_MAINBOARD_NAMES
            .iter()
            .chain(GENERATED_PRINTING_NAMES)
        {
            let id = card_id_by_name(name).expect(name);
            let printing = printing_v1(id).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert!(printing["triggers"].is_array(), "{name}");
        }
        let lookup = |name| printing_v1(card_id_by_name(name).unwrap()).unwrap();
        let force = lookup("Force of Will");
        assert_eq!(force["alt_cost"]["ordered_costs"][0]["PayLife"], 1);
        assert_eq!(
            force["alt_cost"]["ordered_costs"][1]["ExileCardFromHandWithColor"],
            "U"
        );
        assert_eq!(
            lookup("Once Upon a Time")["alt_cost"]["condition"],
            "FirstSpellThisGame"
        );
        assert_eq!(
            lookup("Lion's Eye Diamond")["mana_ability_def"]["amount"],
            3
        );
        assert_eq!(
            lookup("Animate Dead")["attachment"]["AuraCreatureStatic"]["power"],
            -1
        );
        assert!(lookup("Urza's Saga")["saga"].as_array().unwrap().len() == 3);
    }
}
