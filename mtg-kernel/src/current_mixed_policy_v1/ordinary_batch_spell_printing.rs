//! Closed untargeted printed spells. Only checked declarative meanings leave this module.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, DynamicValueDef, Keywords, TargetSpec},
    effect::{CreatureFilter, EffectOp, PlayerRef},
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Seismic Rupture" | "Heroic Reinforcements" | "Brass's Bounty" | "Lunar Insight"
    )
}

/// The caller still applies every shared CardDef exclusion before publication.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let heroic = d.name == "Heroic Reinforcements";
    let bounty = d.name == "Brass's Bounty";
    let lunar = d.name == "Lunar Insight";
    let cost = Cost {
        generic: if bounty { 6 } else { 2 },
        pips: if heroic {
            &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::W)]
        } else if lunar {
            &[Pip::Colored(ManaColor::U)]
        } else {
            &[Pip::Colored(ManaColor::R)]
        },
        x_count: 0,
    };
    let colors: &[ManaColor] = if heroic {
        &[ManaColor::R, ManaColor::W]
    } else if lunar {
        &[ManaColor::U]
    } else {
        &[ManaColor::R]
    };
    let (program, ordered) = if lunar {
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
        || d.types != [CardType::Sorcery]
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
    Ok(Some(json!({"card_id":id,"ordinary_batch_spell_printing":{
        "schema":"printed-untargeted-spell/v1","cost":cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand",
            "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "targets":[],"ordered_resolution":ordered,"no_priority_between_ordered_instructions":true,
            "normal_queued_triggers_publish_after_resolution":true,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
            "countering_skips_all_resolution_instructions":true,"countering_does_not_refund_cost":true,
            "other_cast_permissions_require_current_candidate_authority":true,
            "normal_zone_replacements_apply":true}]
    }})))
}
