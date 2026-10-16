//! Complete ordinary Eaten Alive sorcery printing, independently mandatory
//! for each actor-authorized reference. Native menus and payment stay authoritative.
use super::*;
use crate::card_def::Keywords;
use crate::mana::{Cost, ManaColor, Pip};
fn validate_shape(id: u16, d: &CardDef) -> Result<()> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || d.name != "Eaten Alive"
        || CARD_DEFS
            .get(usize::from(id))
            .is_none_or(|actual| actual.name != d.name)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let crate::card_def::CardDef {
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
    } = d;
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != crate::eaten_alive_v1::PRINTED
        || d.mana_value != 1
        || d.types != [CardType::Sorcery]
        || !d.subtypes.is_empty()
        || d.colors != [ManaColor::B]
        || !d.supertypes.is_empty()
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || !d.activated_abilities.is_empty()
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::CreatureOrPlaneswalker
        || (d.spell_effect)() != Some(crate::eaten_alive_v1::program())
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
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
        || d.additional_cost != Some(crate::eaten_alive_v1::ADDITIONAL)
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
        || d.other_controlled_nontoken_death_v1.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    if !crate::trigger::triggers_for(id).is_empty() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    canonical(id)?;
    Ok(())
}
fn canonical(id: u16) -> Result<crate::rules_meaning_eaten_alive_v1::MeaningV1> {
    let value = crate::rules_meaning_eaten_alive_v1::printed_meaning(id)
        .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    crate::rules_meaning_eaten_alive_v1::validate(&value)
        .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    Ok(value)
}
pub(super) fn baseline(id: u16, d: &CardDef) -> Result<Value> {
    validate_shape(id, d)?;
    Ok(json!({"card_id":id,"definition_owned_family_meanings":[canonical(id)?]}))
}
#[derive(serde::Serialize)]
struct SorceryCharacteristics {
    types: [CardType; 1],
    colors: [ManaColor; 1],
    mana_value: u16,
    token: bool,
}
#[derive(serde::Serialize)]
enum SorceryTiming {
    CasterMainPhaseEmptyStackWithPriority,
}
#[derive(serde::Serialize)]
enum AdditionalBranch {
    SacrificeCurrentControlledCreature {
        count: u8,
        zone: Zone,
        is_target: bool,
        requires_untapped: bool,
        requires_haste: bool,
        bind_original_incarnation_before_payment: bool,
        physical_owner_does_not_select_eligibility: bool,
    },
    PayAdditionalMana(Cost),
}
#[derive(serde::Serialize)]
enum OrderedCost {
    PrintedMana(Cost),
    MandatoryOneOf {
        ordered_branches: [AdditionalBranch; 2],
    },
}
#[derive(serde::Serialize)]
struct TargetBinding {
    count: u8,
    filter: TargetSpec,
    may_be_own_or_opposing: bool,
    bind_original_incarnation_before_payment: bool,
    recheck_source_aware_legality_at_resolution: bool,
    departed_or_reentered_target_is_not_rebound: bool,
}
#[derive(serde::Serialize)]
enum Resolution {
    ExileBoundTarget {
        target_slot: u8,
        requires_same_legal_battlefield_incarnation: bool,
    },
}
#[derive(serde::Serialize)]
struct OrdinarySorceryCastRoot {
    method: crate::state::CastMethodV4,
    ordinary_route: crate::state::SpellCastRouteV4,
    ordinary_origin: Zone,
    timing: SorceryTiming,
    targets: TargetBinding,
    ordered_costs: [OrderedCost; 2],
    targets_chosen_before_additional_branch: bool,
    branch_menu_requires_current_native_combined_payment_quote: bool,
    only_currently_payable_branches_are_offered: bool,
    sole_payable_branch_selected_automatically: bool,
    both_payable_branches_use_original_source_bound_native_option_order: bool,
    multiple_sacrifice_candidates_use_native_cost_target_menu: bool,
    combined_cost_modifiers_require_current_native_payment_quote: bool,
    original_source_and_cost_incarnations_committed_before_responses: bool,
    ordered_resolution: [Resolution; 1],
    all_targets_illegal_skips_resolution_without_refund: bool,
    physical_departure: BaselineDeparture,
    virtual_spell_copy_ceases: bool,
    accepted_spell_copy_pays_no_additional_cost: bool,
    other_cast_permissions_require_current_candidate_authority: bool,
    zone_replacements_still_apply: bool,
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Value> {
    validate_shape(id, d)?;
    Ok(json!({"card_id":id,"registered_sorcery_printing":{
        "characteristics":SorceryCharacteristics {types:[CardType::Sorcery],colors:[ManaColor::B],mana_value:1,token:false},
        "canonical_family_roots":[canonical(id)?],
        "ordered_cast_roots":[OrdinarySorceryCastRoot {
            method:crate::state::CastMethodV4::Normal,ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:SorceryTiming::CasterMainPhaseEmptyStackWithPriority,
            targets:TargetBinding {count:1,filter:TargetSpec::CreatureOrPlaneswalker,may_be_own_or_opposing:true,
                bind_original_incarnation_before_payment:true,recheck_source_aware_legality_at_resolution:true,
                departed_or_reentered_target_is_not_rebound:true},
            ordered_costs:[OrderedCost::PrintedMana(d.cost),OrderedCost::MandatoryOneOf {ordered_branches:[
                AdditionalBranch::SacrificeCurrentControlledCreature {count:1,zone:Zone::Battlefield,is_target:false,
                    requires_untapped:false,requires_haste:false,bind_original_incarnation_before_payment:true,
                    physical_owner_does_not_select_eligibility:true},
                AdditionalBranch::PayAdditionalMana(Cost {generic:3,pips:&[Pip::Colored(ManaColor::B)],x_count:0})
            ]}],
            targets_chosen_before_additional_branch:true,branch_menu_requires_current_native_combined_payment_quote:true,
            only_currently_payable_branches_are_offered:true,sole_payable_branch_selected_automatically:true,
            both_payable_branches_use_original_source_bound_native_option_order:true,
            multiple_sacrifice_candidates_use_native_cost_target_menu:true,
            combined_cost_modifiers_require_current_native_payment_quote:true,
            original_source_and_cost_incarnations_committed_before_responses:true,
            ordered_resolution:[Resolution::ExileBoundTarget {target_slot:0,requires_same_legal_battlefield_incarnation:true}],
            all_targets_illegal_skips_resolution_without_refund:true,
            physical_departure:BaselineDeparture::GraveyardAfterResolutionOrCounter,virtual_spell_copy_ceases:true,
            accepted_spell_copy_pays_no_additional_cost:true,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }]
    }}))
}
#[cfg(test)]
#[path = "tests/eaten_printing.rs"]
mod tests;
