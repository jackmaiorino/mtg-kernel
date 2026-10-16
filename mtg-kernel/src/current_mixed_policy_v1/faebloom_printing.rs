//! Closed Faebloom instant and token recipes. Actual token creation and its
//! separate reflexive trigger remain owned by the existing native program.
use super::*;
use crate::card_def::{Keywords, Subtype};
use crate::mana::{Cost, ManaColor, Pip};
fn validate_shape(id: u16, d: &CardDef) -> Result<bool> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || CARD_DEFS
        .get(usize::from(id))
        .is_none_or(|actual| actual.name != d.name)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let token = match d.name {
        "Faebloom Trick" => false,
        "Faerie Token" => true,
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    let cost = if token {
        Cost::zero()
    } else {
        Cost {
            generic: 2,
            pips: &[Pip::Colored(ManaColor::U)],
            x_count: 0,
        }
    };
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
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != cost
        || d.mana_value != if token { 0 } else { 3 }
        || d.types
            != if token {
                &[CardType::Creature][..]
            } else {
                &[CardType::Instant][..]
            }
        || d.subtypes
            != if token {
                &[Subtype::Faerie][..]
            } else {
                &[][..]
            }
        || d.colors != [ManaColor::U]
        || !d.supertypes.is_empty()
        || d.power != if token { Some(1) } else { None }
        || d.toughness != if token { Some(1) } else { None }
        || d.keywords
            != if token {
                Keywords::FLYING
            } else {
                Keywords::NONE
            }
        || !d.activated_abilities.is_empty()
        || d.is_land
        || d.is_token != token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != if token {
                None
            } else {
                Some(crate::faebloom_trick_v43::recipe())
            }
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
    if token {
        if crate::faebloom_trick_v43::token_definition()
            .map_err(|_| Error::UnsupportedPrintedRoot(id))?
            != id
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    } else {
        meanings(id)?;
    }
    Ok(token)
}
fn meanings(id: u16) -> Result<Vec<crate::rules_meaning_faebloom_v43::MeaningV43>> {
    use crate::rules_meaning_faebloom_v43 as canonical;
    let selector = canonical::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    [
        canonical::RootV43::SpellCreation,
        canonical::RootV43::ReflexiveTap,
    ]
    .into_iter()
    .map(|root| {
        let value = canonical::meaning(&selector, id, root)
            .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        canonical::validate(&selector, &value).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        Ok(value)
    })
    .collect()
}
#[derive(serde::Serialize)]
struct TokenBlueprint {
    card_id: u16,
    characteristics: PermanentCharacteristics,
    no_ordinary_cast_route: bool,
}
fn token_blueprint(requested_id: u16) -> Result<TokenBlueprint> {
    let id = crate::faebloom_trick_v43::token_definition()
        .map_err(|_| Error::UnsupportedPrintedRoot(requested_id))?;
    let d = &CARD_DEFS[usize::from(id)];
    if !validate_shape(id, d)? {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(TokenBlueprint {
        card_id: id,
        characteristics: PermanentCharacteristics {
            types: d.types.to_vec(),
            subtypes: d.subtypes.to_vec(),
            supertypes: Vec::new(),
            colors: d.colors.to_vec(),
            mana_value: 0,
            power: Some(1),
            toughness: Some(1),
            ordered_keywords: vec![PermanentKeyword::Flying],
            minimum_blockers: 1,
            token: true,
            land: false,
            enters_tapped: false,
            printed_mana_abilities: 0,
            printed_activated_abilities: 0,
        },
        no_ordinary_cast_route: true,
    })
}
pub(super) fn baseline(id: u16, d: &CardDef) -> Result<Value> {
    if validate_shape(id, d)? {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(json!({"card_id":id,"definition_owned_family_meanings":meanings(id)?}))
}
#[derive(serde::Serialize)]
struct InstantCharacteristics {
    types: [CardType; 1],
    colors: [ManaColor; 1],
    mana_value: u16,
    token: bool,
}
#[derive(serde::Serialize)]
struct ReflexiveTapRoot {
    mandatory: bool,
    uses_separate_stack_item: bool,
    target_count: u8,
    target_filter: TargetSpec,
    target_selected_only_after_actual_creation: bool,
    already_tapped_target_is_legal: bool,
    controller_and_blue_source_bound_to_original_spell_occurrence: bool,
    current_source_survival_required: bool,
    original_target_incarnation_and_source_aware_legality_rechecked_at_resolution: bool,
    no_legal_target_drops_reflexive_without_undoing_creation: bool,
    ordinary_apnap_placement_priority_responses_and_counterability: bool,
    native_original_occurrence_owns_publication_and_once_only_completion: bool,
}
#[derive(serde::Serialize)]
enum Creation {
    CreateTwoFaeriesThenCaptureOneReflexive {
        proposed_token_count: u8,
        token_blueprint: TokenBlueprint,
        actual_native_token_replacements_and_entry_batch_apply: bool,
        capture_requires_actual_committed_creation: bool,
        replacement_token_count_does_not_multiply_reflexive_occurrences: bool,
        each_accepted_spell_copy_has_distinct_creation_and_reflexive_controller: bool,
    },
}
#[derive(serde::Serialize)]
struct OrdinaryInstantCastRoot {
    method: crate::state::CastMethodV4,
    ordinary_route: crate::state::SpellCastRouteV4,
    ordinary_origin: Zone,
    timing: BaselineTiming,
    target_count: u8,
    targets: TargetSpec,
    ordered_costs: [BaselineManaCost; 1],
    payment_precedes_finalized_spell_and_responses: bool,
    ordered_resolution: [Creation; 1],
    physical_departure: BaselineDeparture,
    virtual_spell_copy_ceases: bool,
    countered_spell_creates_no_tokens_and_no_reflexive: bool,
    other_cast_permissions_require_current_candidate_authority: bool,
    zone_replacements_still_apply: bool,
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Value> {
    if validate_shape(id, d)? {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(json!({"card_id":id,"registered_instant_printing":{
        "characteristics":InstantCharacteristics {types:[CardType::Instant],colors:[ManaColor::U],mana_value:3,token:false},
        "canonical_family_roots":meanings(id)?,
        "ordered_cast_roots":[OrdinaryInstantCastRoot {
            method:crate::state::CastMethodV4::Normal,ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:BaselineTiming::InstantWithPriority,target_count:0,targets:TargetSpec::None,
            ordered_costs:[BaselineManaCost::PrintedMana(d.cost)],payment_precedes_finalized_spell_and_responses:true,
            ordered_resolution:[Creation::CreateTwoFaeriesThenCaptureOneReflexive {
                proposed_token_count:2,token_blueprint:token_blueprint(id)?,actual_native_token_replacements_and_entry_batch_apply:true,
                capture_requires_actual_committed_creation:true,replacement_token_count_does_not_multiply_reflexive_occurrences:true,
                each_accepted_spell_copy_has_distinct_creation_and_reflexive_controller:true,
            }],
            physical_departure:BaselineDeparture::GraveyardAfterResolutionOrCounter,virtual_spell_copy_ceases:true,
            countered_spell_creates_no_tokens_and_no_reflexive:true,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }],
        "ordered_reflexive_roots":[ReflexiveTapRoot {
            mandatory:true,uses_separate_stack_item:true,target_count:1,target_filter:TargetSpec::OpponentControlledCreature,
            target_selected_only_after_actual_creation:true,already_tapped_target_is_legal:true,
            controller_and_blue_source_bound_to_original_spell_occurrence:true,current_source_survival_required:false,
            original_target_incarnation_and_source_aware_legality_rechecked_at_resolution:true,
            no_legal_target_drops_reflexive_without_undoing_creation:true,
            ordinary_apnap_placement_priority_responses_and_counterability:true,
            native_original_occurrence_owns_publication_and_once_only_completion:true,
        }]
    }}))
}
#[cfg(test)]
#[path = "tests/faebloom_printing.rs"]
mod tests;
