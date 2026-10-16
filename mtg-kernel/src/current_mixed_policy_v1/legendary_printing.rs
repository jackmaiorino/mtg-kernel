//! Exact public printing supplements. No serialized executable programs.
use super::*;
use crate::card_def::{
    ActivationTargetFilter, CostComponent, Keywords, Subtype, Supertype, WardCostDef,
};
use crate::mana::{Cost, ManaColor, Pip};
#[derive(serde::Serialize)]
enum StaticRule {
    OtherControlledEffectiveCatCreatures {
        power: i32,
        toughness: i32,
        exclude_source: bool,
    },
    OpponentSpellOrAbilityTargetsSourcePayLifeOrCounter {
        life: u8,
    },
}
#[derive(serde::Serialize)]
enum ActivationCost {
    TapOriginalSource,
}
#[derive(serde::Serialize)]
enum ActivationTiming {
    PriorityAndOrdinaryCreatureTapRestrictions,
}
#[derive(serde::Serialize)]
enum ActivationTargets {
    OneOwnGraveyardEffectiveZombieCreatureCard,
}
#[derive(serde::Serialize)]
enum PermissionRule {
    CastOriginalTargetFromOwnGraveyardThisTurn {
        holder_is_original_activating_controller: bool,
        exact_original_target_incarnation: bool,
        ordinary_cast_timing_and_all_costs: bool,
        source_departure_does_not_revoke: bool,
        cleanup_expires_including_repeated_cleanup: bool,
    },
}
#[derive(serde::Serialize)]
struct Activation {
    printed_index: u8,
    activation_zone: Zone,
    source_must_be_controlled_by_actor: bool,
    timing: ActivationTiming,
    ordered_costs: [ActivationCost; 1],
    targets: ActivationTargets,
    ordered_resolution: [PermissionRule; 1],
}
fn validate_shape(id: u16, d: &CardDef) -> Result<bool> {
    let zul = match d.name {
        "Arahbo, the First Fang" => false,
        "Zul Ashur, Lich Lord" => true,
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    // Exhaustive pattern forces review when CardDef grows.
    let CardDef {
        name: _,
        capability: _,
        cost: _,
        generic_cost_reduction: _,
        ward_cost: _,
        spell_cannot_be_countered: _,
        records_mana_spent: _,
        equipment: _,
        types: _,
        subtypes: _,
        supertypes: _,
        power: _,
        toughness: _,
        starting_loyalty: _,
        enters_with_plus_one_counters: _,
        controlled_counter_keyword: _,
        game_outcome_protection_v1: _,
        controlled_creature_grant: _,
        is_land: _,
        produces_mana: _,
        colors: _,
        target_spec: _,
        keywords: _,
        spell_effect: _,
        mana_ability: _,
        alt_cost: _,
        kicker_cost: _,
        additional_cost: _,
        flashback: _,
        activated_abilities: _,
        plot_cost: _,
        madness_cost: _,
        mode2: _,
        mode3: _,
        additional_modes: _,
        kicked_target_spec: _,
        is_token: _,
        escape: _,
        mana_ability_choices: _,
        enters_battlefield_tapped: _,
        mana_ability_def: _,
        minimum_blockers: _,
        omen: _,
        mana_value: _,
        mana_ability_includes_chosen_color: _,
        as_enters_choose_color_other_than: _,
        additional_mana_abilities: _,
        object_name: _,
        enters_battlefield_tapped_unless: _,
        attachment: _,
        transform_face: _,
        saga: _,
        optional_additional_cost: _,
        changeling: _,
        bestow: _,
        delve: _,
        adventure: _,
        cant_be_blocked_by_monarchs_creatures: _,
        conditional_tap_yield: _,
        enters_tapped_unless_controller: _,
        additional_mana_ability_conditions: _,
        restricted_mana_abilities: _,
        animation: _,
        activated_ability_generic_reductions: _,
        combat_static_rules: _,
        as_enters_color_choice_v1: _,
        chosen_color_creature_boost_v1: _,
        damage_replacement_v1: _,
        token_counter_replacement_v48: _,
        starting_life_self_bonuses_v1: _,
        source_type_hexproof_v1: _,
        other_controlled_nontoken_death_v1: _,
        creature_type_entry_v1: _,
    } = d;
    let cost = Cost {
        generic: if zul { 1 } else { 2 },
        pips: if zul {
            &[Pip::Colored(ManaColor::B)]
        } else {
            &[Pip::Colored(ManaColor::W)]
        },
        x_count: 0,
    };
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || CARD_DEFS
        .get(usize::from(id))
        .is_none_or(|actual| actual.name != d.name)
        || d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != cost
        || d.mana_value != if zul { 2 } else { 3 }
        || d.types != [CardType::Creature]
        || d.subtypes
            != if zul {
                &[Subtype::Zombie, Subtype::Warlock][..]
            } else {
                &[Subtype::Cat, Subtype::Avatar][..]
            }
        || d.supertypes != [Supertype::Legendary]
        || d.colors
            != if zul {
                &[ManaColor::B][..]
            } else {
                &[ManaColor::W][..]
            }
        || d.power != Some(2)
        || d.toughness != Some(2)
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || d.ward_cost
            != if zul {
                Some(WardCostDef::PayLife(2))
            } else {
                None
            }
        || d.generic_cost_reduction.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.attachment.is_some()
        || d.starting_loyalty.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.alt_cost.is_some()
        || d.kicker_cost.is_some()
        || d.additional_cost.is_some()
        || d.flashback.is_some()
        || d.plot_cost.is_some()
        || d.madness_cost.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.kicked_target_spec.is_some()
        || d.escape.is_some()
        || d.enters_battlefield_tapped
        || d.mana_ability_def.is_some()
        || d.omen.is_some()
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || !d.additional_mana_abilities.is_empty()
        || d.enters_battlefield_tapped_unless.is_some()
        || d.transform_face.is_some()
        || d.saga.is_some()
        || d.optional_additional_cost.is_some()
        || d.changeling
        || d.bestow.is_some()
        || d.delve
        || d.adventure.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.conditional_tap_yield.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.animation.is_some()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.as_enters_color_choice_v1.is_some()
        || d.chosen_color_creature_boost_v1.is_some()
        || d.damage_replacement_v1.is_some()
        || d.token_counter_replacement_v48.is_some()
        || !d.starting_life_self_bonuses_v1.is_empty()
        || d.source_type_hexproof_v1.is_some()
        || (d.other_controlled_nontoken_death_v1.is_some() || d.creature_type_entry_v1.is_some())
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    if zul {
        let [a] = d.activated_abilities else {
            return Err(Error::UnsupportedPrintedRoot(id));
        };
        if a.cost != [CostComponent::Tap]
            || a.target_spec != TargetSpec::CreatureCardInOwnGraveyard
            || a.activation_target_filter
                != ActivationTargetFilter::CreatureSubtype(Subtype::Zombie)
            || a.activation_zone != Zone::Battlefield
            || a.sorcery_speed_only
            || a.face.is_some()
            || a.max_activations_per_turn.is_some()
            || (a.effect)() != crate::zul_ashur_v1::recipe()
            || !crate::trigger::triggers_for(id).is_empty()
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    } else if !d.activated_abilities.is_empty() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(zul)
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let zul = validate_shape(id, d)?;
    // Recompute all canonical family roots, never accept a covered-ID/digest
    // as complete rules. Normalization removes hashes from these typed bodies.
    let meanings = if zul {
        use crate::rules_meaning_zul_ashur_v1 as m;
        let s = m::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        [
            m::RootV1::PrintedCreature,
            m::RootV1::WardPayTwoLife,
            m::RootV1::PrintedActivation,
        ]
        .into_iter()
        .map(|r| {
            m::meaning(&s, id, r)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))
                .and_then(|v| serde_json::to_value(v).map_err(|e| Error::Projection(e.to_string())))
        })
        .collect::<Result<Vec<_>>>()?
    } else {
        use crate::rules_meaning_arahbo_v1 as m;
        let s = m::live_selector_v1().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        [
            m::RootV1::OtherControlledCatCreaturesGetPlusOne,
            m::RootV1::ThisOrOtherControlledNontokenCatPermanentCreatesCat,
        ]
        .into_iter()
        .map(|r| {
            m::meaning_v1(&s, id, r)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))
                .and_then(|v| serde_json::to_value(v).map_err(|e| Error::Projection(e.to_string())))
        })
        .collect::<Result<Vec<_>>>()?
    };
    let characteristics = PermanentCharacteristics {
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: d.supertypes.iter().map(|v| *v as u8).collect(),
        colors: d.colors.to_vec(),
        mana_value: d.mana_value,
        power: d.power,
        toughness: d.toughness,
        ordered_keywords: vec![],
        minimum_blockers: d.minimum_blockers,
        token: false,
        land: false,
        enters_tapped: false,
        printed_mana_abilities: 0,
        printed_activated_abilities: if zul { 1 } else { 0 },
    };
    let static_rule = if zul {
        StaticRule::OpponentSpellOrAbilityTargetsSourcePayLifeOrCounter { life: 2 }
    } else {
        StaticRule::OtherControlledEffectiveCatCreatures {
            power: 1,
            toughness: 1,
            exclude_source: true,
        }
    };
    let activation = zul.then_some(Activation {
        printed_index: 0,
        activation_zone: Zone::Battlefield,
        source_must_be_controlled_by_actor: true,
        timing: ActivationTiming::PriorityAndOrdinaryCreatureTapRestrictions,
        ordered_costs: [ActivationCost::TapOriginalSource],
        targets: ActivationTargets::OneOwnGraveyardEffectiveZombieCreatureCard,
        ordered_resolution: [PermissionRule::CastOriginalTargetFromOwnGraveyardThisTurn {
            holder_is_original_activating_controller: true,
            exact_original_target_incarnation: true,
            ordinary_cast_timing_and_all_costs: true,
            source_departure_does_not_revoke: true,
            cleanup_expires_including_repeated_cleanup: true,
        }],
    });
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,"canonical_family_roots":meanings,"ordered_static_rules":[static_rule],"printed_activation":activation,
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {
            method:crate::state::CastMethodV4::Normal,ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(d.cost)],
            ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }]
    }})))
}
