//! Closed targeted entry observers with returns, weakening and optional fight.
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
    )
}

/// The caller still checks all unrelated CardDef fields in the common guard.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (cost, color, subtypes, power, toughness, keywords, target_spec, filter) = match d.name {
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
    let program = if kraken {
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
    if *condition != TriggerCondition::Etb
        || *home_zone != Zone::Battlefield
        || *face_index != 0
        || *intervening_if_kicked
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
    let target = json!({"slot":0,"kind":"Object","filter":filter,"minimum":if pup {0} else {1},"maximum":1,
        "chooser":"CapturedTriggerController","selected_at":"TriggerStackPlacement",
        "normal_target_legality_and_protection_apply":true,"rechecked_at_resolution":true,
        "binding":"OriginalSelectedObjectIncarnation"});
    let resolution = if kraken {
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
    let trigger = json!({"event":"ThisPermanentEntersBattlefield","home_zone":"Battlefield","source_face":0,
        "requires_active_printed_abilities_at_capture":true,"intervening_if":null,"mandatory":true,"uses_stack":true,
        "controller":"CapturedSourceControllerAtEntry","captures_original_source_incarnation":true,
        "survives_source_departure_and_later_control_change":true,"targets":[target],
        "zero_targets_allowed":pup,"zero_targets_perform_no_moves":pup,
        "required_target_absence_prevents_stack_placement":!pup,"all_chosen_targets_illegal_skips_resolution":true,
        "ordered_resolution":resolution});
    Ok(Some(json!({"card_id":id,"targeted_entry_printing":{
        "schema":"printed-targeted-entry/v1","cost":cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,
        "power":power,"toughness":toughness,"keywords":keywords.0,"token":false,
        "ordered_cast_roots":[cast],"ordered_triggers":[trigger]}})))
}
