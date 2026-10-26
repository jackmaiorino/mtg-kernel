//! Closed untargeted printed spells. Only checked declarative meanings leave this module.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, CostComponent, DynamicValueDef, Keywords, TargetSpec},
    effect::{CreatureFilter, EffectOp, PlayerRef},
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Seismic Rupture"
            | "Heroic Reinforcements"
            | "Brass's Bounty"
            | "Lunar Insight"
            | "Goblin Surprise"
            | "Revenge of the Rats"
            | "Inspiring Call"
    )
}

/// The caller still applies every shared CardDef exclusion before publication.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    if d.name == "Goblin Surprise" {
        return goblin_surprise(id, d);
    }
    if d.name == "Revenge of the Rats" {
        return revenge_of_rats(id, d);
    }
    let heroic = d.name == "Heroic Reinforcements";
    let bounty = d.name == "Brass's Bounty";
    let lunar = d.name == "Lunar Insight";
    let inspiring = d.name == "Inspiring Call";
    let cost = Cost {
        generic: if bounty { 6 } else { 2 },
        pips: if heroic {
            &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::W)]
        } else if inspiring {
            &[Pip::Colored(ManaColor::G)]
        } else if lunar {
            &[Pip::Colored(ManaColor::U)]
        } else {
            &[Pip::Colored(ManaColor::R)]
        },
        x_count: 0,
    };
    let colors: &[ManaColor] = if heroic {
        &[ManaColor::R, ManaColor::W]
    } else if inspiring {
        &[ManaColor::G]
    } else if lunar {
        &[ManaColor::U]
    } else {
        &[ManaColor::R]
    };
    let (program, ordered) = if inspiring {
        (
            EffectOp::DrawForCounteredCreaturesThenGrantKeywordUntilEndOfTurn {
                keyword: Keywords::INDESTRUCTIBLE,
            },
            vec![
                json!({"operation":"SnapshotCounteredCreaturesThenDrawAndGrantKeyword",
             "snapshot_time":"ResolutionBeforeFirstDraw","snapshot_filter":{"zone":"Battlefield","controller":"CapturedSpellController","current_effective_type":"Creature","plus_one_plus_one_counter_count_greater_than":0},
             "tokens_and_currently_animated_creatures_may_qualify":true,"other_counter_kinds_and_countered_noncreatures_do_not_qualify":true,
             "each_creature_counts_once_regardless_of_counter_count":true,"bind_exact_current_battlefield_incarnations":true,
             "targets":[],"ordered_instructions":[
                 {"operation":"DrawCards","recipient":"CapturedSpellController","count":"SnapshottedCreatureCount","one_draw_event_per_card":true,"normal_draw_replacements_and_empty_library_rules_apply":true,"drawn_identity_visible_only_to_authorized_viewers":true},
                 {"operation":"GrantKeywordToSnapshottedIncarnations","keyword_bits":Keywords::INDESTRUCTIBLE.0,"duration":"UntilCleanupExpirationAfterDiscard","does_not_resample_after_draws":true,"later_entering_or_later_countered_creatures_not_included":true,"grant_follows_same_incarnation_after_control_change":true,"departed_or_reentered_incarnations_do_not_inherit_grant":true,"normal_continuous_layers_and_ability_removal_apply":true}
             ],"zero_qualifying_creatures_draws_zero_and_grants_nothing":true,"draw_replacement_does_not_cancel_later_keyword_instruction":true}),
            ],
        )
    } else if lunar {
        (
            EffectOp::DrawCardsDynamic {
                player: PlayerRef::Controller,
                count: DynamicValueDef::DistinctManaValuesAmongControlledNonlandPermanents,
            },
            vec![
                json!({"operation":"DrawCards","recipient":"CapturedSpellController",
                "count":{"operation":"CountDistinctManaValuesAmongCurrentControlledNonlandPermanents",
                    "zone":"Battlefield","controller":"CapturedSpellController",
                    "exclude_current_land_type_even_if_also_creature_or_artifact":true,
                    "mana_value":"CurrentNativeObjectManaValue","equal_values_count_once":true,
                    "zero_is_an_ordinary_distinct_value":true,
                    "tokens_are_not_excluded":true,
                    "opponents_permanents_and_nonbattlefield_cards_excluded":true},
                "count_sampled_once_at_resolution_before_drawing":true,
                "empty_set_draws_zero":true,"full_native_integer_count":true,
                "normal_individual_draw_events_replacements_and_empty_library_rules_apply":true}),
            ],
        )
    } else if !heroic && !bounty {
        (
            EffectOp::DamageAllCreatures {
                filter: CreatureFilter::WithoutKeyword(Keywords::FLYING),
                amount: 2,
            },
            vec![
                json!({"operation":"DealNoncombatDamageToCurrentCreatureSet","amount":2,
            "source":"OriginalResolvingSpell","controllers":"BothPlayers",
            "filter":{"lacks_current_effective_keyword":"Flying"},"samples_set_at_resolution":true,
            "simultaneous_damage_batch":true,"does_not_target":true,
            "normal_replacements_prevention_and_state_based_actions_apply":true}),
            ],
        )
    } else {
        let token = crate::card_def::card_id_by_name(if bounty {
            "Treasure Token"
        } else {
            "Soldier Token"
        })
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
        let definition = super::baseline(token)?;
        if bounty {
            (
                EffectOp::CreateTokensDynamic {
                    token_def: token,
                    controller: PlayerRef::Controller,
                    count: DynamicValueDef::ControlledPermanentsWithType(CardType::Land),
                    tapped: false,
                },
                vec![
                    json!({"operation":"CreateTokens","controller":"CapturedSpellController",
                "count":{"operation":"CountCurrentControlledPermanentsWithType","type":"Land",
                    "controller":"CapturedSpellController","tapped_status_irrelevant":true,
                    "not_limited_to_basic_lands":true},
                "count_sampled_once_at_resolution_before_creation":true,"full_native_integer_count":true,
                "token_definition":definition,"entry_tapped":false,
                "normal_token_replacements_and_entry_triggers_apply":true}),
                ],
            )
        } else {
            let create = EffectOp::CreateToken {
                token_def: token,
                controller: PlayerRef::Controller,
            };
            let token_meaning = json!({"operation":"CreateToken","controller":"CapturedSpellController",
                "count":1,"token_definition":definition,"normal_token_replacements_and_entry_triggers_apply":true});
            (
                EffectOp::Sequence(vec![
                    create.clone(),
                    create,
                    EffectOp::BoostControlledCreaturesUntilEndOfTurn {
                        power: 1,
                        toughness: 1,
                        keywords: Keywords::HASTE,
                    },
                ]),
                vec![
                    token_meaning.clone(),
                    token_meaning,
                    json!({"operation":"ModifyCurrentControlledCreatureSet","controller":"CapturedSpellController",
                    "samples_set_after_both_token_instructions":true,"includes_created_soldiers":true,
                    "binds_exact_current_battlefield_incarnations":true,"later_entering_creatures_not_included":true,
                    "power_delta":1,"toughness_delta":1,"keyword_bits":Keywords::HASTE.0,
                    "places_no_counters":true,"duration":"UntilCleanupExpirationAfterDiscard",
                    "normal_continuous_layers_and_timestamped_ability_removal_apply":true}),
                ],
            )
        }
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.types
            != [if inspiring {
                CardType::Instant
            } else {
                CardType::Sorcery
            }]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != colors
        || usize::from(d.mana_value) != usize::from(cost.generic) + cost.pips.len()
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
        || (d.spell_effect)() != Some(program)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand",
            "timing":if inspiring{"InstantWithPriority"}else{"OwnMainPhaseEmptyStackWithPriority"},"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "targets":[],"ordered_resolution":ordered,"no_priority_between_ordered_instructions":true,
            "normal_queued_triggers_publish_after_resolution":true,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
            "countering_skips_all_resolution_instructions":true,"countering_does_not_refund_cost":true,
            "other_cast_permissions_require_current_candidate_authority":true,
            "normal_zone_replacements_apply":true}]);
    Ok(Some(json!({"card_id":id,"ordinary_batch_spell_printing":{
        "schema":"printed-untargeted-spell/v1","cost":cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":cast_roots
    }})))
}

fn revenge_of_rats(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let cost = Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)],
        x_count: 0,
    };
    let token =
        crate::card_def::card_id_by_name("Rat Token").ok_or(Error::UnsupportedPrintedRoot(id))?;
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 4
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
        || d.mode2.is_some()
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || !d
            .flashback
            .as_ref()
            .is_some_and(|f| f.cost == [CostComponent::Mana(cost)])
        || (d.spell_effect)()
            != Some(EffectOp::CreateTokensDynamic {
                token_def: token,
                controller: PlayerRef::Controller,
                count: DynamicValueDef::ControllerGraveyardCardsWithType(CardType::Creature),
                tapped: true,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let token_printing = super::baseline(token)?;
    let resolution = json!([{"operation":"CreateTokens","controller":"CapturedSpellController",
        "count":{"operation":"CountCreatureCardsInOwnGraveyard","owner":"CapturedSpellController",
            "zone":"Graveyard","card_types":"CurrentGraveyardCardDefinitionTypes",
            "excludes_tokens_and_opponents_cards":true,
            "artifact_creature_cards_are_included":true,"noncreature_cards_are_excluded":true},
        "sample_count_once_at_resolution_before_any_creation":true,
        "responses_that_change_graveyard_membership_change_the_count":true,
        "zero_count_creates_no_objects_or_entry_events":true,"full_native_integer_count":true,
        "token_definition":token_printing,"entry_tapped":true,
        "entire_set_initialized_and_committed_as_one_simultaneous_creation_batch":true,
        "tapped_entry_is_set_before_any_entry_observation":true,
        "normal_token_replacements_and_entry_triggers_apply":true}]);
    let roots = [false, true].into_iter().map(|flashback| json!({
        "method":if flashback {"Flashback"} else {"Normal"},
        "route":if flashback {"GraveyardFlashback"} else {"Hand"},
        "origin":if flashback {"OwnGraveyard"} else {"OwnHand"},"origin_owner_must_be_caster":true,
        "timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],
        "ordered_costs":[{"operation":if flashback {"PayFlashbackAlternativeMana"} else {"PayPrintedMana"},
            "cost":cost,"normal_cast_cost_adjustments_and_mana_restrictions_apply":true}],
        "ordered_resolution":resolution,
        "physical_stack_departure":if flashback {"ExileInsteadOfAnyOtherDeparture"} else {"GraveyardAfterResolutionOrCounter"},
        "countering_skips_token_creation_without_refunding_mana":true,
        "normal_queued_triggers_and_state_based_actions_follow_complete_resolution":true,
        "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
        "other_cast_permissions_require_current_candidate_authority":true,
        "normal_zone_replacements_apply":true})).collect::<Vec<_>>();
    Ok(Some(
        json!({"card_id":id,"graveyard_rat_flashback_printing":{
            "schema":"printed-creature-graveyard-count-rats-flashback/v1","cost":cost,"flashback_cost":cost,
            "mana_value":4,"flashback_is_alternative_cost_and_does_not_change_mana_value":true,
            "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
            "power":null,"toughness":null,"keywords":0,"token":false,"ordered_cast_roots":roots
        }}),
    ))
}

fn goblin_surprise(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let cost = Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    let token = crate::card_def::card_id_by_name("Goblin Token")
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    let create = EffectOp::CreateTokensDynamic {
        token_def: token,
        controller: PlayerRef::Controller,
        count: DynamicValueDef::Fixed(2),
        tapped: false,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 3
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
        || d.target_spec != TargetSpec::None
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::BoostControlledCreaturesUntilEndOfTurn {
                power: 2,
                toughness: 0,
                keywords: Keywords::NONE,
            })
        || !d
            .mode2
            .as_ref()
            .is_some_and(|mode| mode.target_spec == TargetSpec::None && (mode.effect)() == create)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let token_meaning = super::baseline(token)?;
    let modes = json!([{"native_index":0,"targets":[],"ordered_resolution":[{
                "operation":"ModifyCurrentControlledCreatureSet","controller":"CapturedSpellController",
                "samples_set_once_at_resolution":true,"binds_exact_current_battlefield_incarnations":true,
                "power_delta":2,"toughness_delta":0,"keyword_bits":0,"places_no_counters":true,
                "includes_current_tokens_and_animated_creatures":true,
                "opposing_creatures_noncreature_permanents_and_later_entries_excluded":true,
                "departed_and_reentered_incarnations_do_not_retain_modification":true,
                "duration":"UntilCleanupExpirationAfterDiscard",
                "normal_continuous_layers_and_control_changes_apply":true}]},
                {"native_index":1,"targets":[],"ordered_resolution":[{
                    "operation":"CreateTokens","controller":"CapturedSpellController","count":2,
                    "token_definition":token_meaning,"entry_tapped":false,
                    "entry_batch":"SimultaneousBeforeEntryObservations",
                    "normal_token_replacements_and_entry_triggers_apply":true}],
                    "new_tokens_gain_no_haste_or_other_mode_bonus_from_this_spell":true}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand",
            "timing":"InstantWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "targets":[],"mode_selection":{"chooser":"CastingPlayer","choose_exactly":1,
                "native_indices":[0,1],"before_targeting_and_final_payment":true,
                "selected_mode_bound_to_original_pending_cast_and_stack_item":true,
                "empty_creature_set_does_not_remove_first_mode":true},
            "ordered_modes":modes,
            "no_priority_between_ordered_instructions":true,
            "normal_queued_triggers_and_state_based_actions_follow_resolution":true,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
            "countering_skips_selected_mode":true,"countering_does_not_refund_cost":true,
            "copy_uses_original_native_bound_mode":true,
            "other_cast_permissions_require_current_candidate_authority":true,
            "normal_zone_replacements_apply":true}]);
    Ok(Some(json!({"card_id":id,"ordinary_batch_spell_printing":{
        "schema":"printed-modal-untargeted-spell/v1","cost":cost,"mana_value":3,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":cast_roots
    }})))
}
