//! Closed player-target spell meanings. Native programs remain private.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, TargetSpec},
    effect::{EffectCond, EffectOp, TargetRef},
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Hidetsugu's Second Rite" | "Exsanguinate")
}

/// Shared CardDef exclusions still run before the caller publishes this value.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    if d.name == "Exsanguinate" {
        return exsanguinate(id, d);
    }
    let cost = Cost {
        generic: 3,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    let program = EffectOp::Conditional {
        cond: EffectCond::TargetPlayerLifeTotalEquals { index: 0, life: 10 },
        then: Box::new(EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 10,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 4
        || d.types != [CardType::Instant]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::R]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::AnyPlayer
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(program)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast_resolution = json!([{"operation":"Conditional",
                    "predicate":{"operation":"TargetPlayerCurrentLifeEquals","target_slot":0,"value":10,
                        "sample_time":"Resolution","exact_full_native_signed_integer_equality":true},
                    "then":[{"operation":"DealNoncombatDamage","target_slot":0,"amount":10,
                        "source":"OriginalResolvingSpell","normal_damage_replacements_and_prevention_apply":true}],
                    "else":[],"condition_is_not_a_cast_or_target_selection_restriction":true,
                    "no_priority_between_predicate_and_damage":true}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand",
                "timing":"InstantWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
                "target_count":1,"target_slot":0,"target_kind":"Player",
                "target_controller_relation":"EitherPlayerIncludingCaster",
                "target_has_no_life_total_restriction_at_cast":true,
                "recheck_original_player_target_legality_at_resolution":true,
                "all_targets_illegal_skips_entire_resolution":true,
                "ordered_resolution":cast_resolution,
                "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
                "zone_destination_applies_when_game_continues":true,
                "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
                "countering_skips_all_resolution_instructions":true,"countering_does_not_refund_cost":true,
                "other_cast_permissions_require_current_candidate_authority":true,
                "normal_zone_replacements_and_state_based_actions_apply":true}]);
    Ok(Some(
        json!({"card_id":id,"conditional_player_spell_printing":{
            "schema":"printed-conditional-player-spell/v1","cost":cost,"mana_value":4,
            "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
            "power":null,"toughness":null,"keywords":0,"token":false,
            "ordered_cast_roots":cast_roots
        }}),
    ))
}

fn exsanguinate(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let cost = Cost {
        generic: 0,
        pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)],
        x_count: 1,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 2
        || d.types != [CardType::Sorcery]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::B]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::None
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(EffectOp::LoseOpponentsLifeXThenGainLifeLost)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(json!({"card_id":id,"exsanguinate_printing":{
        "schema":"printed-x-opponent-life-loss/v1","cost":cost,"printed_mana_value":2,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand",
            "origin_owner_must_be_caster":true,"timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],
            "chosen_x":{"chooser":"CastingPlayer","minimum":0,"maximum":u16::MAX,
                "binding":"OriginalNativeCastXValue","source_is_original_current_cast_choice":true,
                "choice_precedes_final_cost_payment":true,"zero_is_ordinary_choice":true,
                "every_original_native_candidate_is_preserved":true},
            "ordered_costs":[{"operation":"PayNativeAdjustedPrintedMana","printed_cost":cost,
                "generic_x_coefficient":1,"normal_increases_and_reductions_apply":true,"colored_pips_unchanged":true}],
            "mana_value":{"on_stack":"TwoPlusChosenX","elsewhere":2,"native_mana_value_ceiling":u16::MAX},
            "ordered_resolution":[{"operation":"EachOpponentLosesLife","amount":"OriginalChosenX",
                    "relation":"OpponentsOfCapturedSpellController","does_not_target":true,"is_not_damage":true,
                    "loss_not_clipped_to_current_life":true,"normal_life_loss_replacements_apply":true},
                {"operation":"GainLife","recipient":"CapturedSpellController",
                    "amount":"SumActualLifeLostByOpponentsToImmediatelyPrecedingInstruction",
                    "uses_committed_life_loss_not_requested_x_or_current_life_difference":true,
                    "normal_life_gain_replacements_and_triggers_apply":true}],
            "zero_x_loses_and_gains_zero_life":true,"state_based_actions_follow_complete_resolution":true,
            "countering_skips_all_resolution_instructions":true,"countering_does_not_refund_cost":true,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone","copied_spell_uses_its_native_bound_x":true,
            "other_cast_permissions_require_current_candidate_authority":true,"normal_zone_replacements_apply":true}]
    }})))
}
