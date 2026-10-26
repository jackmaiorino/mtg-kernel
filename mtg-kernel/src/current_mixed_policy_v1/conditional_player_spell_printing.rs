//! Closed player-target spell meanings. Native programs remain private.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, TargetSpec},
    effect::{EffectCond, EffectOp, PlayerRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Hidetsugu's Second Rite" | "Exsanguinate" | "Pilfer" | "Seeker's Folly"
    )
}

/// Shared CardDef exclusions still run before the caller publishes this value.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    if d.name == "Exsanguinate" {
        return exsanguinate(id, d);
    }
    if d.name == "Pilfer" {
        return pilfer(id, d);
    }
    if d.name == "Seeker's Folly" {
        return seekers_folly(id, d);
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

fn seekers_folly(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let cost = Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::B)],
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 3
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
        || d.target_spec != TargetSpec::TargetOpponent
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::DiscardCards {
                player: PlayerRef::Target(0),
                count: 2,
            })
        || !d.mode2.as_ref().is_some_and(|m| {
            m.target_spec == TargetSpec::None
                && (m.effect)()
                    == EffectOp::BoostPlayerCreaturesUntilEndOfTurn {
                        player: PlayerRef::Opponent,
                        power: -1,
                        toughness: -1,
                        keywords: Keywords::NONE,
                    }
        })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let modes = json!([{"native_index":0,"target_count":1,"target_slot":0,"target_kind":"OpponentPlayer",
                "caster_is_not_legal_target":true,"empty_hand_is_legal_target":true,"recheck_original_player_target_legality_at_resolution":true,
                "all_targets_illegal_skips_entire_resolution":true,
                "ordered_resolution":[{"operation":"DiscardCards","player":"OriginalTargetPlayer","requested_count":2,
                    "clamp_to_available_hand":true,"chooser":"DiscardingPlayer","choice_is_private_to_chooser":true,
                    "all_card_types_are_eligible":true,"casting_player_does_not_choose_or_see_unrevealed_hand":true,
                    "discard_selected_cards_as_one_batch":true,"zero_available_cards_requires_no_choice":true,
                    "is_cost":false,"normal_discard_and_zone_replacements_apply":true}]},
                {"native_index":1,"targets":[],"ordered_resolution":[{"operation":"ModifyCurrentOpponentCreatureSet",
                    "opponents_of":"CapturedSpellController","samples_set_once_at_resolution":true,
                    "binds_exact_current_battlefield_incarnations":true,"power_delta":-1,"toughness_delta":-1,"keyword_bits":0,
                    "places_no_counters":true,"later_entries_noncreatures_and_casters_creatures_excluded":true,
                    "departed_and_reentered_incarnations_do_not_retain_modification":true,
                    "duration":"UntilCleanupExpirationAfterDiscard","normal_continuous_layers_and_control_changes_apply":true}]}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","timing":"OwnMainPhaseEmptyStackWithPriority",
            "mode_selection":{"chooser":"CastingPlayer","choose_exactly":1,"native_indices":[0,1],"before_targeting_and_final_payment":true,
                "selected_mode_bound_to_original_pending_cast_and_stack_item":true,"empty_hand_or_creature_set_does_not_remove_mode":true},
            "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "ordered_modes":modes,
            "no_priority_during_selected_mode":true,"normal_triggers_and_state_based_actions_follow_resolution":true,
            "countering_skips_selected_mode":true,"countering_does_not_refund_cost":true,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard","virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
            "copy_uses_original_native_bound_mode":true,"other_cast_permissions_require_current_candidate_authority":true}]);
    Ok(Some(json!({"card_id":id,"seekers_folly_printing":{
        "schema":"printed-opponent-discard-or-creature-weakening/v1","cost":cost,"mana_value":3,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,"power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":cast_roots
    }})))
}

fn pilfer(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let cost = Cost {
        generic: 1,
        pips: &[Pip::Colored(ManaColor::B)],
        x_count: 0,
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
        || d.target_spec != TargetSpec::TargetOpponent
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::RevealTargetHandChooseNonlandDiscard {
                player: PlayerRef::Target(0),
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let resolution = json!([{"operation":"RevealEntireCurrentTargetPlayerHand","target_slot":0,"audience":"BothPlayers",
                "capture_time":"Resolution","binds_every_current_hand_card_and_original_zone_incarnation":true,
                "does_not_reveal_future_draws_or_other_hidden_zones":true,
                "public_identity_knowledge_is_retained_only_under_normal_native_card_knowledge_rules":true},
                {"operation":"ChooseNonlandCardFromThatRevealedHand","chooser":"CapturedSpellController",
                    "required_count_if_eligible":1,"zero_eligible_cards_skips_choice":true,"cannot_decline_when_nonempty":true,
                    "creature_cards_are_eligible":true,"land_cards_are_ineligible_even_if_also_other_types":true,
                    "full_current_native_candidate_order_and_zone_incarnations_preserved":true},
                {"operation":"TargetPlayerDiscardsChosenCard","target_slot":0,"count":1,
                    "binding":"OriginalChosenHandCardIncarnation","is_cost":false,"normal_discard_and_zone_replacements_apply":true}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","timing":"OwnMainPhaseEmptyStackWithPriority",
            "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"target_count":1,"target_slot":0,
            "target_kind":"OpponentPlayer","caster_is_not_legal_target":true,"empty_hand_is_legal_target":true,
            "recheck_original_player_target_legality_at_resolution":true,"all_targets_illegal_skips_entire_resolution":true,
            "ordered_resolution":resolution,
            "no_priority_between_reveal_choice_and_discard":true,
            "countering_skips_reveal_choice_and_discard":true,"countering_does_not_refund_cost":true,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
            "normal_triggers_and_state_based_actions_follow_resolution":true,
            "other_cast_permissions_require_current_candidate_authority":true}]);
    Ok(Some(json!({"card_id":id,"pilfer_printing":{
        "schema":"printed-revealed-opponent-hand-nonland-discard/v1","cost":cost,"mana_value":2,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,"power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":cast_roots
    }})))
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
    let resolution = json!([{"operation":"EachOpponentLosesLife","amount":"OriginalChosenX",
                    "relation":"OpponentsOfCapturedSpellController","does_not_target":true,"is_not_damage":true,
                    "loss_not_clipped_to_current_life":true,"normal_life_loss_replacements_apply":true},
                {"operation":"GainLife","recipient":"CapturedSpellController",
                    "amount":"SumActualLifeLostByOpponentsToImmediatelyPrecedingInstruction",
                    "uses_committed_life_loss_not_requested_x_or_current_life_difference":true,
                    "normal_life_gain_replacements_and_triggers_apply":true}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand",
            "origin_owner_must_be_caster":true,"timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],
            "chosen_x":{"chooser":"CastingPlayer","minimum":0,"maximum":u16::MAX,
                "binding":"OriginalNativeCastXValue","source_is_original_current_cast_choice":true,
                "choice_precedes_final_cost_payment":true,"zero_is_ordinary_choice":true,
                "every_original_native_candidate_is_preserved":true},
            "ordered_costs":[{"operation":"PayNativeAdjustedPrintedMana","printed_cost":cost,
                "generic_x_coefficient":1,"normal_increases_and_reductions_apply":true,"colored_pips_unchanged":true}],
            "mana_value":{"on_stack":"TwoPlusChosenX","elsewhere":2,"native_mana_value_ceiling":u16::MAX},
            "ordered_resolution":resolution,
            "zero_x_loses_and_gains_zero_life":true,"state_based_actions_follow_complete_resolution":true,
            "countering_skips_all_resolution_instructions":true,"countering_does_not_refund_cost":true,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone","copied_spell_uses_its_native_bound_x":true,
            "other_cast_permissions_require_current_candidate_authority":true,"normal_zone_replacements_apply":true}]);
    Ok(Some(json!({"card_id":id,"exsanguinate_printing":{
        "schema":"printed-x-opponent-life-loss/v1","cost":cost,"printed_mana_value":2,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":cast_roots
    }})))
}
