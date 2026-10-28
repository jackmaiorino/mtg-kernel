//! Closed Divine ordinary and kicker printing. Canonical V44 owns the exact
//! controlled-creature cardinalities and per-incarnation indestructible grants.
use super::*;
use crate::card_def::Keywords;
use crate::mana::{Cost, ManaColor, Pip};
fn validate_shape(id: u16, d: &CardDef) -> Result<()> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || CARD_DEFS
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
        creature_type_entry_v1: _,
    } = d;
    if d.name != "Divine Resilience"
        || d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost
            != (Cost {
                generic: 0,
                pips: &[Pip::Colored(ManaColor::W)],
                x_count: 0,
            })
        || d.mana_value != 1
        || d.types != [CardType::Instant]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::W]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || !d.activated_abilities.is_empty()
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::KickerControlledCreatures
        || (d.spell_effect)() != Some(crate::rules_meaning_divine_v44::recipe())
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
        || d.kicker_cost
            != Some(Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::W)],
                x_count: 0,
            })
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

    if !crate::trigger::triggers_for(id).is_empty() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    meaning(id)?;
    Ok(())
}
fn meaning(id: u16) -> Result<crate::rules_meaning_divine_v44::MeaningV44> {
    use crate::rules_meaning_divine_v44 as canonical;
    let selector = canonical::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    let value = canonical::meaning(&selector, id).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    canonical::validate(&value).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    Ok(value)
}
pub(super) fn baseline(id: u16, d: &CardDef) -> Result<Value> {
    validate_shape(id, d)?;
    Ok(json!({"card_id":id,"definition_owned_family_meanings":[meaning(id)?]}))
}
#[derive(serde::Serialize)]
struct InstantCharacteristics {
    types: [CardType; 1],
    colors: [ManaColor; 1],
    mana_value: u16,
    token: bool,
}
#[derive(serde::Serialize)]
enum CostStep {
    PrintedMana(Cost),
    AdditionalKickerMana(Cost),
}
#[derive(serde::Serialize)]
struct OrdinaryInstantCastRoot {
    method: crate::state::CastMethodV4,
    ordinary_route: crate::state::SpellCastRouteV4,
    ordinary_origin: Zone,
    origin_owner_must_be_caster: bool,
    timing: BaselineTiming,
    kicked: bool,
    ordered_costs: Vec<CostStep>,
    target_spec: TargetSpec,
    minimum_targets: u64,
    maximum_targets: Option<u64>,
    distinct_targets_without_fixed_width_limit: bool,
    explicit_finish_after_any_kicked_target_prefix: bool,
    kicker_choice_precedes_target_selection: bool,
    kicker_choice_offered_only_if_total_cost_payable: bool,
    decline_requires_complete_unkicked_target_assignment: bool,
    payment_precedes_finalized_spell_and_responses: bool,
    ordered_resolution: [crate::rules_meaning_divine_v44::MeaningV44; 1],
    original_incarnation_current_controller_and_source_aware_legality_rechecked: bool,
    captured_spell_controller_and_kicker_own_resolution: bool,
    physical_departure: BaselineDeparture,
    virtual_spell_copy_ceases: bool,
    countered_spell_executes_no_instructions: bool,
    other_cast_permissions_require_current_candidate_authority: bool,
    zone_replacements_still_apply: bool,
}
fn cast(id: u16, d: &CardDef, kicked: bool) -> Result<OrdinaryInstantCastRoot> {
    let mut costs = vec![CostStep::PrintedMana(d.cost)];
    if kicked {
        costs.push(CostStep::AdditionalKickerMana(
            d.kicker_cost.ok_or(Error::UnsupportedPrintedRoot(id))?,
        ));
    }
    Ok(OrdinaryInstantCastRoot {
        method: crate::state::CastMethodV4::Normal,
        ordinary_route: crate::state::SpellCastRouteV4::Hand,
        ordinary_origin: Zone::Hand,
        origin_owner_must_be_caster: true,
        timing: BaselineTiming::InstantWithPriority,
        kicked,
        ordered_costs: costs,
        target_spec: TargetSpec::KickerControlledCreatures,
        minimum_targets: if kicked { 0 } else { 1 },
        maximum_targets: if kicked { None } else { Some(1) },
        distinct_targets_without_fixed_width_limit: true,
        explicit_finish_after_any_kicked_target_prefix: kicked,
        kicker_choice_precedes_target_selection: true,
        kicker_choice_offered_only_if_total_cost_payable: true,
        decline_requires_complete_unkicked_target_assignment: true,
        payment_precedes_finalized_spell_and_responses: true,
        ordered_resolution: [meaning(id)?],
        original_incarnation_current_controller_and_source_aware_legality_rechecked: true,
        captured_spell_controller_and_kicker_own_resolution: true,
        physical_departure: BaselineDeparture::GraveyardAfterResolutionOrCounter,
        virtual_spell_copy_ceases: true,
        countered_spell_executes_no_instructions: true,
        other_cast_permissions_require_current_candidate_authority: true,
        zone_replacements_still_apply: true,
    })
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Value> {
    validate_shape(id, d)?;
    Ok(json!({"card_id":id,"registered_instant_printing":{
        "characteristics":InstantCharacteristics {types:[CardType::Instant],colors:[ManaColor::W],mana_value:1,token:false},
        "ordered_cast_roots":[cast(id,d,false)?,cast(id,d,true)?],
    }}))
}
#[cfg(test)]
#[path = "tests/divine_printing.rs"]
mod tests;
