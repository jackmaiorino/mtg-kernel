//! Closed attack observers. Programs authenticate the printing and stay private.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, DynamicValueDef, Keywords, Subtype, TargetSpec},
    effect::{EffectCond, EffectOp, ObjectRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::TriggerCondition,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Armasaur Guide"
            | "Battlesong Berserker"
            | "Crypt Feaster"
            | "Dauntless Veteran"
            | "Courageous Goblin"
    )
}

/// The caller additionally enforces every unrelated CardDef field.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (generic, pips, color, subtypes, power, toughness, keywords) = match d.name {
        "Armasaur Guide" => (
            4,
            &[Pip::Colored(ManaColor::W)][..],
            ManaColor::W,
            &[Subtype::Dinosaur][..],
            4,
            4,
            Keywords::VIGILANCE,
        ),
        "Battlesong Berserker" => (
            3,
            &[Pip::Colored(ManaColor::R)][..],
            ManaColor::R,
            &[Subtype::Human, Subtype::Berserker][..],
            3,
            4,
            Keywords::NONE,
        ),
        "Crypt Feaster" => (
            3,
            &[Pip::Colored(ManaColor::B)][..],
            ManaColor::B,
            &[Subtype::Zombie][..],
            3,
            4,
            Keywords::MENACE,
        ),
        "Dauntless Veteran" => (
            1,
            &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)][..],
            ManaColor::W,
            &[Subtype::Human, Subtype::Soldier][..],
            2,
            2,
            Keywords::NONE,
        ),
        "Courageous Goblin" => (
            1,
            &[Pip::Colored(ManaColor::R)][..],
            ManaColor::R,
            &[Subtype::Goblin][..],
            2,
            2,
            Keywords::NONE,
        ),
        _ => return Ok(None),
    };
    let cost = Cost {
        generic,
        pips,
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(generic) + pips.len()
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors != [color]
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != keywords
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::None
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
    let bound_boost =
        |power, toughness| EffectOp::BindTemporaryBoostToTriggerSource { power, toughness };
    let boost_meaning = |power, toughness| {
        json!({"operation":"ModifyOriginalTriggerSourcePowerToughness",
        "power_delta":power,"toughness_delta":toughness,"layer":"7c",
        "binding":"CapturedSourceObjectAndBattlefieldIncarnation","requires_same_battlefield_incarnation":true,
        "missing_or_reentered_source_gets_no_boost":true,"duration":"UntilCleanupExpirationAfterDiscard"})
    };
    let target_spec = if matches!(d.name, "Armasaur Guide" | "Battlesong Berserker") {
        TargetSpec::ControlledCreature
    } else {
        TargetSpec::None
    };
    let (condition, event, program, ordered, intervening) = match d.name {
        "Armasaur Guide" => (
            TriggerCondition::ControllerAttacksWithAtLeastCreatures(3),
            json!({"event":"ObserverControllerDeclaresAttackers","minimum_declared_creatures":3,
                "trigger_count_per_declaration":1,"observer_need_not_attack":true,"put_into_combat_is_not_declaration":true}),
            EffectOp::AddCountersToTarget {
                target_index: 0,
                optional: false,
                plus1_plus1: 1,
                lifelink: 0,
                stun: 0,
            },
            vec![
                json!({"operation":"PutPlusOnePlusOneCounter","count":1,"recipient":"OriginalLegalTarget",
                "normal_counter_replacements_and_triggers_apply":true}),
            ],
            Value::Null,
        ),
        "Battlesong Berserker" => (
            TriggerCondition::ControllerAttacks,
            json!({"event":"ObserverControllerDeclaresAttackers","minimum_declared_creatures":1,
                "trigger_count_per_declaration":1,"observer_need_not_attack":true,"put_into_combat_is_not_declaration":true}),
            EffectOp::Sequence(vec![
                EffectOp::PumpTargetUntilEndOfTurnDynamic {
                    target: TargetRef::Target(0),
                    power: DynamicValueDef::Fixed(1),
                    toughness: DynamicValueDef::Fixed(0),
                },
                EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::Target(0),
                    keyword: Keywords::MENACE,
                },
            ]),
            vec![
                json!({"operation":"ModifyOriginalLegalTargetPowerToughness","power_delta":1,"toughness_delta":0,
                    "layer":"7c","duration":"UntilCleanupExpirationAfterDiscard"}),
                json!({"operation":"GrantKeywordToOriginalLegalTarget","keyword":"Menace","layer":6,
                    "duration":"UntilCleanupExpirationAfterDiscard"}),
            ],
            Value::Null,
        ),
        "Crypt Feaster" => (
            TriggerCondition::AttacksWithControllerGraveyardCardCountAtLeast(7),
            json!({"event":"OriginalSourceDeclaredAttacker","controller_graveyard_card_count_at_least":7,
                "tokens_do_not_count":true,"checks_at_capture":true}),
            EffectOp::Conditional {
                cond: EffectCond::ControllerGraveyardCardCountAtLeast(7),
                then: Box::new(bound_boost(2, 0)),
                else_: Box::new(EffectOp::Sequence(vec![])),
            },
            vec![boost_meaning(2, 0)],
            json!({"condition":"CapturedTriggerControllerGraveyardHasAtLeastCards",
                "count":7,"tokens_do_not_count":true,"recheck_at_resolution":true,"false_skips_resolution":true}),
        ),
        "Dauntless Veteran" => (
            TriggerCondition::Attacks,
            json!({"event":"OriginalSourceDeclaredAttacker"}),
            EffectOp::BoostControlledCreaturesUntilEndOfTurn {
                power: 1,
                toughness: 1,
                keywords: Keywords::NONE,
            },
            vec![
                json!({"operation":"ModifyControlledCreatureSetPowerToughness","controller":"CapturedTriggerController",
                "select_set_at_resolution":true,"current_effective_creatures_only":true,"includes_source_if_still_qualifying":true,
                "includes_nontargetable_creatures":true,"power_delta":1,"toughness_delta":1,"layer":"7c",
                "bind_each_selected_incarnation":true,"does_not_affect_later_entrants":true,
                "duration":"UntilCleanupExpirationAfterDiscard"}),
            ],
            Value::Null,
        ),
        "Courageous Goblin" => (
            TriggerCondition::AttacksWhileControllerHasPowerFourCreature,
            json!({"event":"OriginalSourceDeclaredAttacker","controller_has_current_effective_creature_power_at_least":4,
                "source_may_satisfy_power_condition":true,"checks_at_capture_only":true,"does_not_recheck_power_at_resolution":true}),
            EffectOp::Sequence(vec![
                bound_boost(1, 0),
                EffectOp::Conditional {
                    cond: EffectCond::SourceStillInTriggerZone,
                    then: Box::new(EffectOp::GrantKeywordTargetUntilEndOfTurn {
                        object: ObjectRef::ThisSource,
                        keyword: Keywords::MENACE,
                    }),
                    else_: Box::new(EffectOp::Sequence(vec![])),
                },
            ]),
            vec![
                boost_meaning(1, 0),
                json!({"operation":"GrantKeywordToOriginalTriggerSource","keyword":"Menace",
                "binding":"CapturedSourceObjectAndBattlefieldIncarnation","requires_same_battlefield_incarnation":true,
                "layer":6,"missing_or_reentered_source_gets_no_keyword":true,"duration":"UntilCleanupExpirationAfterDiscard"}),
            ],
            Value::Null,
        ),
        _ => unreachable!(),
    };
    let [trigger] = crate::trigger::triggers_for(id) else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    if trigger.condition != condition
        || trigger.home_zone != Zone::Battlefield
        || trigger.face_index != 0
        || trigger.intervening_if_kicked
        || trigger.intervening_if_controls_another_source_card
        || (trigger.effect)() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(target_spec)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let targets = if target_spec == TargetSpec::None {
        json!([])
    } else {
        json!([{
            "slot":0,"kind":"Object","minimum":1,"maximum":1,"chooser":"CapturedTriggerController",
            "selected_at":"TriggerStackPlacement","filter":{"zone":"Battlefield","current_effective_type":"Creature",
                "controller":"CapturedTriggerController"},"binding":"OriginalSelectedObjectIncarnation",
            "normal_target_legality_and_protection_apply":true,"rechecked_at_resolution":true
        }])
    };
    Ok(Some(json!({"card_id":id,"attack_trigger_printing":{
        "schema":"printed-attack-observer/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,
        "subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":power,"toughness":toughness,
        "keywords":keywords.0,"token":false,
        "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
            "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "targets":[],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
            "countering_skips_entry_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
            "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true}],
        "ordered_triggers":[{"event":event,"home_zone":"Battlefield","source_face":0,
            "requires_active_printed_abilities_at_capture":true,"intervening_if":intervening,"mandatory":true,"uses_stack":true,
            "controller":"ObserverControllerAtEvent","captures_source_incarnation_and_controller_at_event":true,
            "survives_source_departure_and_later_control_change":true,"targets":targets,
            "required_target_absence_prevents_stack_placement":target_spec != TargetSpec::None,
            "all_chosen_targets_illegal_skips_resolution":target_spec != TargetSpec::None,
            "ordered_resolution":ordered}]
    }})))
}
