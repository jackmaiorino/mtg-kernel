//! Closed ordinary Kellan printing and canonical, definition-owned upgrades.
//! Only the native owner authenticates current eligibility, payment and grants.
use super::*;
use crate::card_def::{ActivationTargetFilter, CostComponent, Keywords, Subtype};
use crate::mana::{Cost, ManaColor, Pip};
use crate::standard_creatures_v1::CreatureEffectV1;
const PRINTED: Cost = Cost {
    generic: 0,
    pips: &[Pip::Colored(ManaColor::R)],
    x_count: 0,
};
fn validate_shape(id: u16, d: &CardDef) -> Result<()> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || d.name != "Kellan, Planar Trailblazer"
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
        creature_type_entry_v1: _,
    } = d;
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != PRINTED
        || d.mana_value != 1
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Human, Subtype::Faerie, Subtype::Scout]
        || d.colors != [ManaColor::R]
        || d.supertypes != [crate::card_def::Supertype::Legendary]
        || d.power != Some(2)
        || d.toughness != Some(1)
        || d.keywords != Keywords::NONE
        || d.activated_abilities.len() != 2
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

    for (index, a) in d.activated_abilities.iter().enumerate() {
        let cost = Cost {
            generic: if index == 0 { 1 } else { 2 },
            ..PRINTED
        };
        if !matches!(a.cost,[CostComponent::Mana(actual)]if *actual==cost)
            || a.target_spec != TargetSpec::None
            || a.activation_zone != Zone::Battlefield
            || a.sorcery_speed_only
            || a.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
            || a.max_activations_per_turn.is_some()
            || a.face.is_some()
            || (a.effect)()
                != EffectOp::CreatureUpgrade(if index == 0 {
                    CreatureEffectV1::KellanDetective
                } else {
                    CreatureEffectV1::KellanRogue
                })
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    }
    let triggers = crate::trigger::triggers_for(id);
    if triggers.len() != 1 {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let a = &triggers[0];
    if a.condition != crate::trigger::TriggerCondition::DealsCombatDamageToPlayer
        || a.home_zone != Zone::Battlefield
        || a.face_index != 0
        || a.intervening_if_kicked
        || a.intervening_if_controls_another_source_card
        || crate::trigger::target_spec_for_trigger(id, &(a.effect)()) != Some(TargetSpec::None)
        || (a.effect)()
            != (EffectOp::ImpulseDraw {
                count: 1,
                duration: crate::effect::ImpulseDuration::EndOfTurn,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(())
}
#[derive(serde::Serialize)]
enum Layer {
    TypeChanging4,
    AbilityAdding6,
    BasePowerToughness7b,
}
#[derive(serde::Serialize)]
enum UpgradeStep {
    ReplaceCreatureSubtypes {
        types: [Subtype; 3],
        preserve_noncreature_subtypes: bool,
        layer: Layer,
    },
    GrantCombatDamagePlayerImpulse {
        layer: Layer,
    },
    SetBasePowerToughness {
        power: i16,
        toughness: i16,
        layer: Layer,
    },
    GrantDoubleStrike {
        layer: Layer,
    },
}
#[derive(serde::Serialize)]
struct UpgradeRoot {
    printed_ability_index: u8,
    activation_zone: Zone,
    ordered_costs: [BaselineManaCost; 1],
    target_count: u8,
    current_source_controller_must_be_actor: bool,
    instant_speed_with_priority: bool,
    requires_tap_or_haste: bool,
    requires_active_effective_printed_slot: bool,
    uses_stack_and_can_be_countered: bool,
    prerequisite_effective_subtype_at_resolution: Subtype,
    prerequisite_is_not_activation_restriction: bool,
    failed_condition_or_departed_source_keeps_payment_without_effect: bool,
    original_source_incarnation_bound_before_payment: bool,
    ordered_resolution: Vec<UpgradeStep>,
    timestamp_ordered_layers: bool,
    persists_through_cleanup: bool,
    source_departure_ends_upgrade_and_reentry_starts_printed: bool,
    rogue_upgrade_preserves_previously_granted_combat_impulse: bool,
}
#[derive(serde::Serialize)]
struct CombatImpulseRoot {
    native_definition_trigger_index: u8,
    printed_table_row_requires_live_resolution_grant: bool,
    trigger_requires_actual_combat_damage_to_player_and_active_incarnation_grant: bool,
    each_distinct_combat_damage_event_triggers_once: bool,
    double_strike_can_trigger_in_each_damage_step: bool,
    captured_current_source_controller_is_trigger_controller_and_library_owner: bool,
    original_host_incarnation_and_grant_source_preserved_in_private_native_contracts: bool,
    target_count: u8,
    uses_stack_priority_responses_and_counterability: bool,
    source_departure_or_control_change_after_capture_does_not_redirect_trigger: bool,
    exile_top_cards: u8,
    empty_library_exiles_nothing_without_draw_loss: bool,
    permission_holder_is_captured_trigger_controller: bool,
    permission_binds_exact_exiled_incarnation: bool,
    may_play_land_or_cast_spell: bool,
    ordinary_timing_land_limits_costs_and_other_restrictions_apply: bool,
    permission_pays_mana_normally: bool,
    permission_expires_at_current_turn_cleanup: bool,
    exiled_card_remains_exiled_after_permission_expires: bool,
}
#[derive(serde::Serialize)]
struct PrintedMeaning {
    ordered_upgrade_roots: [UpgradeRoot; 2],
    granted_combat_impulse: CombatImpulseRoot,
}
fn meaning(id: u16) -> Result<PrintedMeaning> {
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    validate_shape(id, d)?;
    let root = |index, subtype, steps| UpgradeRoot {
        printed_ability_index: index,
        activation_zone: Zone::Battlefield,
        ordered_costs: [BaselineManaCost::PrintedMana(Cost {
            generic: if index == 0 { 1 } else { 2 },
            ..PRINTED
        })],
        target_count: 0,
        current_source_controller_must_be_actor: true,
        instant_speed_with_priority: true,
        requires_tap_or_haste: false,
        requires_active_effective_printed_slot: true,
        uses_stack_and_can_be_countered: true,
        prerequisite_effective_subtype_at_resolution: subtype,
        prerequisite_is_not_activation_restriction: true,
        failed_condition_or_departed_source_keeps_payment_without_effect: true,
        original_source_incarnation_bound_before_payment: true,
        ordered_resolution: steps,
        timestamp_ordered_layers: true,
        persists_through_cleanup: true,
        source_departure_ends_upgrade_and_reentry_starts_printed: true,
        rogue_upgrade_preserves_previously_granted_combat_impulse: true,
    };
    Ok(PrintedMeaning {
        ordered_upgrade_roots: [
            root(
                0,
                Subtype::Scout,
                vec![
                    UpgradeStep::ReplaceCreatureSubtypes {
                        types: [Subtype::Human, Subtype::Faerie, Subtype::Detective],
                        preserve_noncreature_subtypes: true,
                        layer: Layer::TypeChanging4,
                    },
                    UpgradeStep::GrantCombatDamagePlayerImpulse {
                        layer: Layer::AbilityAdding6,
                    },
                ],
            ),
            root(
                1,
                Subtype::Detective,
                vec![
                    UpgradeStep::ReplaceCreatureSubtypes {
                        types: [Subtype::Human, Subtype::Faerie, Subtype::Rogue],
                        preserve_noncreature_subtypes: true,
                        layer: Layer::TypeChanging4,
                    },
                    UpgradeStep::SetBasePowerToughness {
                        power: 3,
                        toughness: 2,
                        layer: Layer::BasePowerToughness7b,
                    },
                    UpgradeStep::GrantDoubleStrike {
                        layer: Layer::AbilityAdding6,
                    },
                ],
            ),
        ],
        granted_combat_impulse: CombatImpulseRoot {
            native_definition_trigger_index: 0,
            printed_table_row_requires_live_resolution_grant: true,
            trigger_requires_actual_combat_damage_to_player_and_active_incarnation_grant: true,
            each_distinct_combat_damage_event_triggers_once: true,
            double_strike_can_trigger_in_each_damage_step: true,
            captured_current_source_controller_is_trigger_controller_and_library_owner: true,
            original_host_incarnation_and_grant_source_preserved_in_private_native_contracts: true,
            target_count: 0,
            uses_stack_priority_responses_and_counterability: true,
            source_departure_or_control_change_after_capture_does_not_redirect_trigger: true,
            exile_top_cards: 1,
            empty_library_exiles_nothing_without_draw_loss: true,
            permission_holder_is_captured_trigger_controller: true,
            permission_binds_exact_exiled_incarnation: true,
            may_play_land_or_cast_spell: true,
            ordinary_timing_land_limits_costs_and_other_restrictions_apply: true,
            permission_pays_mana_normally: true,
            permission_expires_at_current_turn_cleanup: true,
            exiled_card_remains_exiled_after_permission_expires: true,
        },
    })
}
pub(super) fn baseline(id: u16, d: &CardDef) -> Result<Value> {
    validate_shape(id, d)?;
    Ok(json!({"card_id":id,"definition_owned_family_meanings":[meaning(id)?]}))
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    validate_shape(id, d)?;
    let characteristics = PermanentCharacteristics {
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: d.supertypes.iter().map(|v| *v as u8).collect(),
        colors: d.colors.to_vec(),
        mana_value: 1,
        power: Some(2),
        toughness: Some(1),
        ordered_keywords: Vec::new(),
        minimum_blockers: 1,
        token: false,
        land: false,
        enters_tapped: false,
        printed_mana_abilities: 0,
        printed_activated_abilities: 2,
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,"canonical_family_roots":[meaning(id)?],
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {
            method:crate::state::CastMethodV4::Normal,ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(PRINTED)],
            ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }]
    }})))
}
#[cfg(test)]
#[path = "tests/kellan_printing.rs"]
mod tests;
