//! Exact library-search printings. Private resolution selections are not targets.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, LibraryCardFilter, LibrarySearchDestinationV1, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};
pub(super) fn handles_permanent(name: &str) -> bool {
    matches!(name, "Campus Guide" | "Rune-Scarred Demon" | "Micromancer")
}
pub(super) fn handles_spell(name: &str) -> bool {
    matches!(name, "Grim Tutor" | "Bushwhack")
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles_permanent(d.name) && !handles_spell(d.name) {
        return Ok(None);
    }
    if d.name == "Bushwhack" {
        return bushwhack(id, d);
    }
    let campus = d.name == "Campus Guide";
    let micromancer = d.name == "Micromancer";
    let spell = handles_spell(d.name);
    let cost = Cost {
        generic: if campus {
            2
        } else if spell {
            1
        } else if micromancer {
            3
        } else {
            5
        },
        pips: if campus {
            &[]
        } else if micromancer {
            &[Pip::Colored(ManaColor::U)]
        } else {
            &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
        },
        x_count: 0,
    };
    let types = if campus {
        &[CardType::Artifact, CardType::Creature][..]
    } else if spell {
        &[CardType::Sorcery][..]
    } else {
        &[CardType::Creature][..]
    };
    let subtypes = if campus {
        &[Subtype::Golem][..]
    } else if spell {
        &[][..]
    } else if micromancer {
        &[Subtype::Human, Subtype::Wizard][..]
    } else {
        &[Subtype::Demon][..]
    };
    let power = if campus {
        Some(2)
    } else if spell {
        None
    } else if micromancer {
        Some(3)
    } else {
        Some(6)
    };
    let toughness = if campus {
        Some(1)
    } else if spell {
        None
    } else if micromancer {
        Some(3)
    } else {
        Some(6)
    };
    let keywords = if !campus && !spell && !micromancer {
        Keywords::FLYING
    } else {
        Keywords::NONE
    };
    let search = if campus {
        EffectOp::SearchLibraryCardsToDestination {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::BasicLand,
            max_targets: 1,
            destination: LibrarySearchDestinationV1::LibraryTopAfterShuffle,
        }
    } else {
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: if micromancer {
                LibraryCardFilter::InstantOrSorceryManaValue(1)
            } else {
                LibraryCardFilter::AnyCard
            },
        }
    };
    let program = if campus {
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![EffectOp::Sequence(vec![]), search],
        }
    } else if micromancer {
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![search, EffectOp::Sequence(vec![])],
        }
    } else if spell {
        EffectOp::Sequence(vec![
            search,
            EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 3,
            },
        ])
    } else {
        search
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(cost.generic) + cost.pips.len()
        || d.types != types
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors
            != if campus {
                &[][..]
            } else if micromancer {
                &[ManaColor::U][..]
            } else {
                &[ManaColor::B][..]
            }
        || d.power != power
        || d.toughness != toughness
        || d.keywords != keywords
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(if spell {
                program.clone()
            } else {
                EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                }
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    if spell {
        if !crate::trigger::triggers_for(id).is_empty() {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    } else {
        let [trigger] = crate::trigger::triggers_for(id) else {
            return Err(Error::UnsupportedPrintedRoot(id));
        };
        let TriggeredAbilityDef {
            condition,
            home_zone,
            face_index,
            intervening_if_kicked,
            intervening_if_controls_another_source_card,
            effect,
        } = trigger;
        if *condition != TriggerCondition::Etb
            || *home_zone != Zone::Battlefield
            || *face_index != 0
            || *intervening_if_kicked
            || *intervening_if_controls_another_source_card
            || effect() != program
            || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
            || crate::trigger::unselected_trigger_modes(id, &program).is_some()
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    }
    let select = json!({"operation":"SearchOwnLibrary","player":"CapturedResolutionController","selection_is_not_targeting":true,
        "chooser":"CapturedResolutionController","choice_visibility":"ChooserOnly","scope":"EntireCurrentOwnLibrary",
        "filter":if campus{json!("BasicLandCard")}else if micromancer{json!({"any_front_face_type":["Instant","Sorcery"],"printed_mana_value_equals":1})}else{json!("AnyCard")},"maximum":1,
        "minimum":if campus || micromancer{"ZeroEvenWhenMatchesExist"}else{"OneIfLibraryNonemptyOtherwiseZero"},
        "binds_exact_live_selected_incarnation":true,"may_not_select_another_players_library":true,
        "selected_card_revealed":campus || micromancer,"normal_zone_change_replacements_apply":true});
    let resolution = if campus {
        json!([{"operation":"ChooseWhetherToSearch","chooser":"CapturedResolutionController","at":"TriggerResolution","options":[
            {"index":0,"ordered_instructions":[],"searches":false,"shuffles":false,"preserves_library_order_knowledge_and_randomness":true},
            {"index":1,"ordered_instructions":[select,{"operation":"RevealSelectedCardIfAny"},{"operation":"ShuffleOwnLibrary","required_even_when_no_card_found":true},{"operation":"PutSelectedCardOnTopAfterShuffleIfAny","top_identity_known_to_both_players":true}]}]}])
    } else if micromancer {
        json!([{"operation":"ChooseWhetherToSearch","chooser":"CapturedResolutionController","at":"TriggerResolution","options":[
            {"index":0,"ordered_instructions":[select,{"operation":"MoveSelectedCardToOwnersHandIfAny","reveal":false},{"operation":"RevealSelectedHandCardToBothPlayersIfAny","only_if_selected_object_successfully_ended_in_owners_hand":true},{"operation":"ShuffleOwnLibrary","required_even_when_no_card_found":true}]},
            {"index":1,"ordered_instructions":[],"searches":false,"shuffles":false,"preserves_library_order_knowledge_and_randomness":true}]}])
    } else {
        let mut instructions = vec![
            select,
            json!({"operation":"MoveSelectedCardToOwnersHandIfAny","reveal":false}),
            json!({"operation":"ShuffleOwnLibrary","required_even_when_library_empty":true}),
        ];
        if spell {
            instructions.push(json!({"operation":"LoseLife","player":"CapturedResolutionController","amount":3,"not_damage":true,"follows_search_and_shuffle_even_when_library_empty":true}));
        }
        Value::Array(instructions)
    };
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
        "ordered_resolution":if spell{resolution.clone()}else{json!([{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}])},
        "countering_skips_resolution_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
        "physical_spell_departure":if spell{"OwnersGraveyardAfterResolutionOrCounter"}else{"BattlefieldOnResolutionOtherwiseOwnersGraveyard"},
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let triggers = if spell {
        json!([])
    } else {
        json!([{"event":"ThisPermanentEntersBattlefield","controller":"CapturedSourceControllerAtEntry","home_zone":"Battlefield","source_face":0,"mandatory_trigger":true,"uses_stack":true,"targets":[],"requires_active_printed_abilities_at_capture":true,"survives_source_departure_and_later_control_change":true,"ordered_resolution":resolution}])
    };
    Ok(Some(
        json!({"card_id":id,"library_search_printing":{"schema":"printed-owned-library-search/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":power,"toughness":toughness,"keywords":keywords.0,"token":false,"ordered_cast_roots":[cast],"ordered_triggers":triggers}}),
    ))
}

fn bushwhack(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let cost = Cost {
        generic: 0,
        pips: &[Pip::Colored(ManaColor::G)],
        x_count: 0,
    };
    let search = EffectOp::SearchLibraryToHand {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::BasicLand,
    };
    let fight = EffectOp::FightObjects {
        first: ObjectRef::Target(0),
        second: ObjectRef::Target(1),
        target_spec: TargetSpec::ControlledCreatureThenOpponentCreature,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 1
        || d.types != [CardType::Sorcery]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::G]
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
        || (d.spell_effect)() != Some(search)
        || !d.mode2.as_ref().is_some_and(|m| {
            m.target_spec == TargetSpec::ControlledCreatureThenOpponentCreature
                && (m.effect)() == fight
        })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let search_mode = json!({"native_index":0,"targets":[],"ordered_resolution":[{
        "operation":"SearchOwnLibraryForBasicLandCardToHand","searcher":"CapturedSpellController",
        "filter":{"card_type":"Land","supertype":"Basic"},"choose_zero_or_one":true,
        "may_fail_to_find_even_when_matching_card_exists":true,
        "search_and_unselected_identities_private_to_searcher":true,
        "reveal_selected_card_before_moving_to_searchers_hand":true,
        "shuffle_remaining_library_after_selection_or_failure":true,
        "empty_library_still_completes_search_and_shuffle_without_drawing":true,
        "selection_is_resolution_choice_not_spell_target":true,
        "normal_zone_replacements_apply":true}]});
    let fight_mode = json!({"native_index":1,"target_count":2,"ordered_targets":[
        {"slot":0,"zone":"Battlefield","effective_type":"Creature","controller":"Caster"},
        {"slot":1,"zone":"Battlefield","effective_type":"Creature","controller":"OpponentOfCaster"}],
        "bind_and_recheck_each_original_incarnation_type_controller_and_targeting_restriction":true,
        "illegal_slots_never_compacted_or_reassigned":true,
        "ordered_resolution":[{"operation":"Fight","creature_source_slots":[0,1],
            "requires_both_targets_still_legal":true,"either_illegal_target_prevents_both_damage_packets":true,
            "sample_both_current_effective_powers_before_either_damage_packet":true,
            "negative_or_zero_power_deals_no_damage":true,"damage_class":"Noncombat",
            "damage_sources_are_the_two_original_creatures_not_the_spell":true,
            "recipients_are_each_other":true,"one_simultaneous_damage_batch":true,
            "current_source_keywords_protection_replacements_and_prevention_apply":true,
            "no_intermediate_state_based_actions_between_damage_packets":true}]});
    let casts = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
        "mode_selection":{"chooser":"CastingPlayer","choose_exactly":1,"native_indices":[0,1],
            "before_targeting_and_final_payment":true,"fight_requires_a_complete_legal_target_pair":true,
            "search_mode_remains_available_with_empty_library_or_no_basic_lands":true,
            "selected_mode_bound_to_original_pending_cast_and_stack_item":true},
        "ordered_modes":[search_mode,fight_mode],"all_targets_illegal_skips_entire_selected_mode":true,
        "normal_queued_triggers_and_state_based_actions_follow_complete_resolution":true,
        "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
        "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
        "countering_skips_selected_mode_without_refunding_cost":true,
        "other_cast_permissions_require_current_candidate_authority":true}]);
    Ok(Some(json!({"card_id":id,"modal_search_fight_printing":{
        "schema":"printed-modal-basic-land-search-or-fight/v1","cost":cost,"mana_value":1,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"token":false,"ordered_cast_roots":casts
    }})))
}
