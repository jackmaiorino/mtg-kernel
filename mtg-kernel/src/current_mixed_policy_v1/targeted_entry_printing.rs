//! Closed targeted entry and landfall observers.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, DynamicValueDef, Keywords, Subtype, TargetSpec},
    effect::{EffectCond, EffectOp, ObjectRef, PlayerRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Mischievous Pup"
            | "Bigfin Bouncer"
            | "Elvish Regrower"
            | "Vampire Soulcaller"
            | "Tragic Banshee"
            | "Affectionate Indrik"
            | "Grappling Kraken"
            | "Felidar Savior"
            | "Sun-Blessed Healer"
    )
}

/// The caller still checks all unrelated CardDef fields in the common guard.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (cost, color, subtypes, power, toughness, keywords, target_spec, filter) = match d.name {
        "Sun-Blessed Healer" => (
            Cost {
                generic: 1,
                pips: &[Pip::Colored(ManaColor::W)],
                x_count: 0,
            },
            ManaColor::W,
            &[Subtype::Human, Subtype::Cleric][..],
            3,
            1,
            Keywords::LIFELINK,
            TargetSpec::NonlandPermanentCardInOwnGraveyardManaValueAtMost(2),
            json!({"zone":"GraveyardOfCapturedTriggerController","kind":"Card","printed_permanent_type":true,"exclude_land_type":true,"printed_mana_value_at_most":2,"not_limited_to_creatures":true}),
        ),
        "Felidar Savior" => (
            Cost {
                generic: 3,
                pips: &[Pip::Colored(ManaColor::W)],
                x_count: 0,
            },
            ManaColor::W,
            &[Subtype::Cat, Subtype::Beast][..],
            2,
            3,
            Keywords::LIFELINK,
            TargetSpec::UpToTwoOtherControlledCreatures,
            json!({"zone":"Battlefield","type":"Creature","relation":"ControlledByCapturedTriggerController","excludes_original_source_incarnation":true,"uses_current_effective_type":true}),
        ),
        "Grappling Kraken" => (
            Cost {
                generic: 4,
                pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)],
                x_count: 0,
            },
            ManaColor::U,
            &[Subtype::Kraken][..],
            5,
            6,
            Keywords::NONE,
            TargetSpec::OpponentControlledCreature,
            json!({"zone":"Battlefield","type":"Creature","relation":"ControlledByOpponentOfCapturedTriggerController","uses_current_effective_type":true}),
        ),
        "Affectionate Indrik" => (
            Cost {
                generic: 5,
                pips: &[Pip::Colored(ManaColor::G)],
                x_count: 0,
            },
            ManaColor::G,
            &[Subtype::Beast][..],
            4,
            4,
            Keywords::NONE,
            TargetSpec::OpponentControlledCreature,
            json!({"zone":"Battlefield","type":"Creature","relation":"ControlledByOpponentOfCapturedTriggerController","uses_current_effective_type":true}),
        ),
        "Tragic Banshee" => (
            Cost {
                generic: 4,
                pips: &[Pip::Colored(ManaColor::B)],
                x_count: 0,
            },
            ManaColor::B,
            &[Subtype::Spirit][..],
            5,
            3,
            Keywords::NONE,
            TargetSpec::OpponentControlledCreature,
            json!({"zone":"Battlefield","type":"Creature","relation":"ControlledByOpponentOfCapturedTriggerController","uses_current_effective_type":true}),
        ),
        "Mischievous Pup" => (
            Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::W)],
                x_count: 0,
            },
            ManaColor::W,
            &[Subtype::Dog][..],
            3,
            1,
            Keywords::FLASH,
            TargetSpec::UpToOneOtherControlledPermanent,
            json!({"zone":"Battlefield","relation":"ControlledByCapturedTriggerController","excludes_original_source_object":true}),
        ),
        "Bigfin Bouncer" => (
            Cost {
                generic: 3,
                pips: &[Pip::Colored(ManaColor::U)],
                x_count: 0,
            },
            ManaColor::U,
            &[Subtype::Shark, Subtype::Pirate][..],
            3,
            2,
            Keywords::NONE,
            TargetSpec::OpponentControlledCreature,
            json!({"zone":"Battlefield","type":"Creature","relation":"ControlledByOpponentOfCapturedTriggerController","uses_current_effective_type":true}),
        ),
        "Elvish Regrower" => (
            Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)],
                x_count: 0,
            },
            ManaColor::G,
            &[Subtype::Elf, Subtype::Druid][..],
            4,
            3,
            Keywords::NONE,
            TargetSpec::PermanentCardInOwnGraveyard,
            json!({"zone":"GraveyardOfCapturedTriggerController","kind":"Card","has_any_card_type":["Artifact","Creature","Enchantment","Land","Planeswalker"],"excludes_tokens_and_virtual_spell_copies":true}),
        ),
        "Vampire Soulcaller" => (
            Cost {
                generic: 4,
                pips: &[Pip::Colored(ManaColor::B)],
                x_count: 0,
            },
            ManaColor::B,
            &[Subtype::Vampire, Subtype::Warlock][..],
            3,
            2,
            Keywords::FLYING,
            TargetSpec::CreatureCardInOwnGraveyard,
            json!({"zone":"GraveyardOfCapturedTriggerController","kind":"Card","type":"Creature"}),
        ),
        _ => return Ok(None),
    };
    let pup = d.name == "Mischievous Pup";
    let banshee = d.name == "Tragic Banshee";
    let indrik = d.name == "Affectionate Indrik";
    let kraken = d.name == "Grappling Kraken";
    let felidar = d.name == "Felidar Savior";
    let healer = d.name == "Sun-Blessed Healer";
    let optional_targets = pup || felidar;
    let program = if healer {
        EffectOp::Conditional {
            cond: EffectCond::WasKicked,
            then: Box::new(EffectOp::ReturnTargetPermanentToBattlefield { target_index: 0 }),
            else_: Box::new(EffectOp::Sequence(vec![])),
        }
    } else if felidar {
        EffectOp::Sequence(
            (0..2)
                .map(|target_index| EffectOp::Conditional {
                    cond: EffectCond::TargetIsLegalForAbility {
                        index: target_index,
                        spec: target_spec,
                    },
                    then: Box::new(EffectOp::AddCountersToTarget {
                        target_index,
                        optional: true,
                        plus1_plus1: 1,
                        lifelink: 0,
                        stun: 0,
                    }),
                    else_: Box::new(EffectOp::Sequence(vec![])),
                })
                .collect(),
        )
    } else if kraken {
        EffectOp::Sequence(vec![
            EffectOp::TapObject {
                object: ObjectRef::Target(0),
            },
            EffectOp::AddCountersToTarget {
                target_index: 0,
                optional: false,
                plus1_plus1: 0,
                lifelink: 0,
                stun: 1,
            },
        ])
    } else if indrik {
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![
                EffectOp::Sequence(vec![]),
                EffectOp::FightObjects {
                    first: ObjectRef::ThisSource,
                    second: ObjectRef::Target(0),
                    target_spec: TargetSpec::OpponentControlledCreature,
                },
            ],
        }
    } else if banshee {
        let weaken = |amount| EffectOp::PumpTargetUntilEndOfTurnDynamic {
            target: TargetRef::Target(0),
            power: DynamicValueDef::Fixed(amount),
            toughness: DynamicValueDef::Fixed(amount),
        };
        EffectOp::Conditional {
            cond: EffectCond::CreatureDiedThisTurn,
            then: Box::new(weaken(-13)),
            else_: Box::new(weaken(-1)),
        }
    } else if d.name == "Bigfin Bouncer" {
        EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Hand,
        }
    } else {
        EffectOp::MoveAllTargets {
            to_zone: Zone::Hand,
        }
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(cost.generic) + cost.pips.len()
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors != [color]
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != keywords
        || d.kicker_cost != if healer { Some(cost) } else { None }
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
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
    if *condition
        != if kraken {
            TriggerCondition::ControlledLandEnters
        } else {
            TriggerCondition::Etb
        }
        || *home_zone != Zone::Battlefield
        || *face_index != 0
        || *intervening_if_kicked != healer
        || *intervening_if_controls_another_source_card
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(target_spec)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":if pup {"InstantWithPriority"} else {"OwnMainPhaseEmptyStackWithPriority"},
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let casts = if healer {
        [false,true].into_iter().map(|kicked|{
        let mut root=cast.clone();root["kicked"]=json!(kicked);
        root["ordered_costs"]=json!([{"operation":"PayTotalManaCost","printed_mana_component":cost,
            "chosen_kicker_additional_component":if kicked{Some(cost)}else{None},
            "base_total_before_increases_and_reductions":Cost{generic:if kicked{2}else{1},pips:if kicked{&[Pip::Colored(ManaColor::W),Pip::Colored(ManaColor::W)]}else{cost.pips},x_count:0},
            "one_native_payment_plan":true,"normal_cost_increases_and_reductions_apply_to_total":true}]);
        root["kicker_chosen_before_payment_does_not_change_mana_value"]=json!(true);root
    }).collect::<Vec<_>>()
    } else {
        vec![cast]
    };
    let target = json!({"slot":0,"kind":"Object","filter":filter,"minimum":if optional_targets {0} else {1},"maximum":if felidar {2}else{1},
        "chooser":"CapturedTriggerController","selected_at":"TriggerStackPlacement",
        "normal_target_legality_and_protection_apply":true,"rechecked_at_resolution":true,
        "binding":"OriginalSelectedObjectIncarnation"});
    let resolution = if healer {
        json!([{"operation":"IfCapturedEnteringSpellWasKickedReturnOriginalGraveyardTargetToBattlefield",
            "target_slot":0,"requires_original_target_incarnation_and_complete_current_legality":true,
            "controller":"ReturnedCardsOwner","not_returned_to_hand":true,"not_intrinsically_tapped":true,
            "normal_entry_tap_replacements_and_enters_with_characteristics_apply":true,
            "normal_battlefield_entry_triggers_apply":true,"noncreature_nonland_permanents_are_eligible":true,
            "returning_aura_uses_legal_untargeted_attachment_host_choice_if_needed":true,
            "aura_host_choice_must_obey_enchant_restrictions_and_protection":true,
            "untargeted_aura_entry_does_not_trigger_ward_or_obey_hexproof_or_shroud_targeting_restrictions":true,
            "no_legal_aura_host_leaves_aura_in_graveyard":true,
            "source_departure_or_new_incarnation_does_not_cancel_original_trigger":true,
            "captured_kicker_fact_rechecked_at_resolution_not_future_source_incarnation":true}])
    } else if felidar {
        json!([{"operation":"PutPlusOnePlusOneCounterOnEachLegalChosenCreature","target_slots":[0,1],"count_per_target":1,
            "choose_zero_one_or_two_distinct_other_creatures":true,"missing_optional_slots_skipped":true,
            "each_slot_rechecks_original_incarnation_current_creature_type_control_and_normal_target_legality":true,
            "an_illegal_slot_does_not_cancel_other_legal_slots":true,"exclude_original_source_incarnation":true,
            "counter_placement_mandatory_on_each_legal_chosen_target":true,"retains_existing_counters":true,
            "normal_counter_replacements_apply":true,"permanent_counter_not_until_end_of_turn":true,
            "source_departure_does_not_remove_counters_or_prevent_resolution":true,
            "choosing_zero_places_no_counters":true}])
    } else if kraken {
        json!([{"operation":"TapCreature","target_slot":0,
                "binding":"OriginalSelectedCreatureIncarnation","already_tapped_target_is_legal":true,
                "normal_tapping_rules_apply":true},
            {"operation":"PutStunCountersOnCreature","target_slot":0,"count":1,"mandatory":true,
                "does_not_require_tap_status_to_change":true,
                "retains_existing_counters_and_adds_to_current_stun_count":true,
                "normal_counter_placement_replacements_apply":true,
                "ordinary_stun_rule":"WhenThisPermanentWouldUntapRemoveOneStunCounterInstead",
                "counter_is_not_an_until_end_of_turn_effect":true,
                "source_departure_does_not_remove_counter":true,
                "new_target_incarnation_does_not_inherit_old_counter":true}])
    } else if indrik {
        json!([{"operation":"MayHaveOriginalSourceFightTarget","target_slot":0,
            "chooser":"CapturedTriggerController","choose_at_resolution_after_target_selection":true,
            "native_options":[{"index":0,"operation":"DeclineWithoutEffect"},{"index":1,"operation":"Fight"}],
            "requires_current_original_source_incarnation_and_legal_target_incarnation":true,
            "departed_source_last_known_power_does_not_allow_fighting":true,
            "reentered_source_is_a_new_incarnation_and_cannot_substitute":true,
            "target_type_controller_hexproof_protection_and_incarnation_rechecked":true,
            "sample_both_current_effective_powers_before_either_damage_packet":true,
            "nonpositive_power_deals_no_damage":true,"damage_class":"Noncombat",
            "damage_sources":"OriginalSourceCreatureAndOriginalTargetCreature",
            "recipients_are_each_other":true,"one_simultaneous_damage_batch":true,
            "current_source_keywords_replacements_and_prevention_apply":true,
            "state_based_actions_and_queued_triggers_follow_complete_batch":true}])
    } else if banshee {
        json!([{"operation":"ModifyTargetPowerToughnessUntilEndOfTurn","target_slot":0,
            "binding":"OriginalSelectedCreatureIncarnation",
            "condition":{"operation":"AnyCreatureDiedThisTurn","sample_at_resolution":true,
                "both_controllers_included":true,"requires_battlefield_creature_to_graveyard_move":true,
                "not_an_intervening_if_or_trigger_time_snapshot":true},
            "if_true":{"power_delta":-13,"toughness_delta":-13},
            "if_false":{"power_delta":-1,"toughness_delta":-1},
            "layer":"AdditivePowerToughness","places_no_counters":true,
            "duration":"UntilCleanupExpirationAfterDiscard",
            "source_departure_does_not_cancel_resolved_modification":true,
            "later_target_control_change_retains_same_incarnation_modification":true,
            "target_departure_and_reentry_does_not_retain_modification":true,
            "normal_continuous_layers_and_state_based_actions_apply":true}])
    } else {
        json!([{"operation":"ReturnEachLegalTargetToHand","recipient":"EachTargetsOwner",
            "requires_original_target_incarnation":true,"does_not_return_to_trigger_controllers_hand_unless_owner":true,
            "normal_zone_replacements_apply":true,"tokens_that_leave_the_battlefield_cease_normally":true}])
    };
    let mut trigger = json!({"event":if kraken{"LandEntersUnderObserverControl"}else{"ThisPermanentEntersBattlefield"},"home_zone":"Battlefield","source_face":0,
        "requires_active_printed_abilities_at_capture":true,"intervening_if":null,"mandatory":true,"uses_stack":true,
        "controller":if kraken{"CapturedSourceControllerAtLandEntry"}else{"CapturedSourceControllerAtEntry"},"captures_original_source_incarnation":true,
        "survives_source_departure_and_later_control_change":true,"targets":[target],
        "zero_targets_allowed":optional_targets,"zero_targets_perform_no_moves":optional_targets,
        "required_target_absence_prevents_stack_placement":!optional_targets,"all_chosen_targets_illegal_skips_resolution":true,
        "ordered_resolution":resolution});
    if kraken {
        trigger["landfall"] = json!({"requires_land_play":false,"token_lands_count":true,
            "uses_current_effective_land_type_and_entry_controller":true,"trigger_once_per_entering_land":true,
            "creature_entry_itself_does_not_trigger":true});
    }
    if healer {
        trigger["intervening_if"] = json!({"operation":"EnteringSpellWasKicked","check_at_entry_and_resolution":true,"noncast_entries_are_not_kicked":true,"captures_original_entry_cast_fact":true});
    }
    Ok(Some(json!({"card_id":id,"targeted_entry_printing":{
        "schema":"printed-targeted-entry/v1","cost":cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,
        "power":power,"toughness":toughness,"keywords":keywords.0,"token":false,
        "ordered_cast_roots":casts,"ordered_triggers":[trigger]}})))
}
