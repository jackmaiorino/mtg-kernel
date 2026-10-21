//! Closed battlefield observers. Executable recipes are compared, never encoded.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec},
    effect::{EffectOp, ObjectRef, PlayerRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::TriggerCondition,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Ajani's Pridemate"
            | "Marauding Blight-Priest"
            | "Dazzling Angel"
            | "Youthful Valkyrie"
            | "Spitfire Lagac"
            | "Tatyova, Benthic Druid"
            | "Brineborn Cutthroat"
            | "Elementalist Adept"
            | "Beast-Kin Ranger"
            | "Firebrand Archer"
            | "Crackling Cyclops"
            | "Elfsworn Giant"
    )
}

/// The caller also checks every additional CardDef field through the shared
/// exclusion guard. This module admits exactly one mandatory untargeted trigger.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (generic, pips, colors, subtypes, power, toughness, keywords) = match d.name {
        "Elfsworn Giant" => (
            3,
            &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)][..],
            &[ManaColor::G][..],
            &[Subtype::Giant][..],
            5,
            3,
            Keywords::REACH,
        ),
        "Firebrand Archer" => (
            1,
            &[Pip::Colored(ManaColor::R)][..],
            &[ManaColor::R][..],
            &[Subtype::Human, Subtype::Archer][..],
            2,
            1,
            Keywords::NONE,
        ),
        "Crackling Cyclops" => (
            2,
            &[Pip::Colored(ManaColor::R)][..],
            &[ManaColor::R][..],
            &[Subtype::Cyclops, Subtype::Wizard][..],
            0,
            4,
            Keywords::NONE,
        ),
        "Ajani's Pridemate" => (
            1,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Cat, Subtype::Soldier][..],
            2,
            2,
            Keywords::NONE,
        ),
        "Marauding Blight-Priest" => (
            2,
            &[Pip::Colored(ManaColor::B)][..],
            &[ManaColor::B][..],
            &[Subtype::Vampire, Subtype::Cleric][..],
            3,
            2,
            Keywords::NONE,
        ),
        "Dazzling Angel" => (
            2,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Angel][..],
            2,
            3,
            Keywords::FLYING,
        ),
        "Youthful Valkyrie" => (
            1,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Angel][..],
            1,
            3,
            Keywords::FLYING,
        ),
        "Spitfire Lagac" => (
            3,
            &[Pip::Colored(ManaColor::R)][..],
            &[ManaColor::R][..],
            &[Subtype::Lizard][..],
            3,
            4,
            Keywords::NONE,
        ),
        "Brineborn Cutthroat" => (
            1,
            &[Pip::Colored(ManaColor::U)][..],
            &[ManaColor::U][..],
            &[Subtype::Merfolk, Subtype::Pirate][..],
            2,
            1,
            Keywords::FLASH,
        ),
        "Elementalist Adept" | "Crackling Cyclops" => (
            1,
            &[Pip::Colored(ManaColor::U)][..],
            &[ManaColor::U][..],
            &[Subtype::Human, Subtype::Wizard][..],
            2,
            1,
            Keywords::FLASH,
        ),
        "Beast-Kin Ranger" => (
            2,
            &[Pip::Colored(ManaColor::G)][..],
            &[ManaColor::G][..],
            &[Subtype::Elf, Subtype::Ranger][..],
            3,
            3,
            Keywords::TRAMPLE,
        ),
        "Tatyova, Benthic Druid" => (
            3,
            &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::U)][..],
            &[ManaColor::G, ManaColor::U][..],
            &[Subtype::Merfolk, Subtype::Druid][..],
            3,
            3,
            Keywords::NONE,
        ),
        _ => return Ok(None),
    };
    let cost = Cost {
        generic,
        pips,
        x_count: 0,
    };
    let supertypes = if d.name == "Tatyova, Benthic Druid" {
        &[Supertype::Legendary][..]
    } else {
        &[][..]
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.colors != colors
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || d.supertypes != supertypes
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != keywords
        || usize::from(d.mana_value) != usize::from(generic) + pips.len()
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::None
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
    let gain = || EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 1,
    };
    let gain_meaning = || {
        json!({"operation":"GainLife","recipient":"CapturedTriggerController","amount":1,
        "normal_life_gain_replacements_and_triggers_apply":true})
    };
    let counter_meaning = || {
        json!({"operation":"AddCounterToOriginalTriggerSource","counter":"PlusOnePlusOne","count":1,
        "binding":"CapturedSourceObjectAndBattlefieldIncarnation","requires_same_battlefield_incarnation":true,
        "missing_or_reentered_source_gets_no_counter":true,"normal_counter_replacements_and_triggers_apply":true})
    };
    let boost_meaning = |power, toughness| {
        json!({"operation":"ModifyOriginalTriggerSourcePowerToughness",
        "power_delta":power,"toughness_delta":toughness,"layer":"AdditivePowerToughness",
        "binding":"CapturedSourceObjectAndBattlefieldIncarnation","requires_same_battlefield_incarnation":true,
        "missing_or_reentered_source_gets_no_boost":true,"duration":"UntilCleanupExpirationAfterDiscard"})
    };
    let (condition, event, program, ordered) = match d.name {
        "Brineborn Cutthroat" => (
            TriggerCondition::CastSpellDuringOpponentsTurn,
            json!({"event":"ObserverControllerCastsSpell","requires_opponents_active_turn":true,
                "copies_without_cast_do_not_count":true,"trigger_count_per_cast":1}),
            EffectOp::BindPlusOnePlusOneCounterToTriggerSource,
            vec![counter_meaning()],
        ),
        "Elementalist Adept" => (
            TriggerCondition::CastNoncreatureSpell,
            json!({"event":"ObserverControllerCastsNoncreatureSpell","copies_without_cast_do_not_count":true,
                "current_spell_characteristics":true,"trigger_count_per_cast":1}),
            EffectOp::BindTemporaryBoostToTriggerSource {
                power: if d.name == "Crackling Cyclops" { 3 } else { 1 },
                toughness: if d.name == "Crackling Cyclops" { 0 } else { 1 },
            },
            vec![if d.name == "Crackling Cyclops" {
                boost_meaning(3, 0)
            } else {
                boost_meaning(1, 1)
            }],
        ),
        "Firebrand Archer" => (
            TriggerCondition::CastNoncreatureSpell,
            json!({"event":"ObserverControllerCastsNoncreatureSpell","copies_without_cast_do_not_count":true,
                "current_spell_characteristics":true,"trigger_count_per_cast":1}),
            EffectOp::DealDamage {
                target: TargetRef::Opponent,
                amount: 1,
            },
            vec![
                json!({"operation":"DealDamage","recipient":"OpponentOfCapturedTriggerController","amount":1,
                "source":"OriginalTriggerSource","damage_class":"Noncombat",
                "normal_damage_replacements_and_prevention_apply":true}),
            ],
        ),
        "Ajani's Pridemate" | "Marauding Blight-Priest" => (
            TriggerCondition::ControllerGainsLife,
            json!({"event":"ObserverControllerActuallyGainsLife","requires_positive_actual_gain":true,
                "trigger_count_per_gain_event":1,"does_not_scale_with_life_amount":true}),
            if d.name == "Ajani's Pridemate" {
                EffectOp::BindPlusOnePlusOneCounterToTriggerSource
            } else {
                EffectOp::LoseLife {
                    player: PlayerRef::Opponent,
                    amount: 1,
                }
            },
            if d.name == "Ajani's Pridemate" {
                vec![counter_meaning()]
            } else {
                vec![
                    json!({"operation":"LoseLife","recipient":"OpponentOfCapturedTriggerController","amount":1,
                    "is_damage":false,"normal_life_loss_replacements_apply":true}),
                ]
            },
        ),
        "Dazzling Angel" | "Youthful Valkyrie" | "Beast-Kin Ranger" => (
            TriggerCondition::OtherControlledCreatureEnters {
                subtype: if d.name == "Youthful Valkyrie" {
                    Some(Subtype::Angel)
                } else {
                    None
                },
            },
            json!({"event":"AnotherCreatureEntersUnderObserverControl","excludes_this_source":true,
                "required_current_subtype":if d.name == "Youthful Valkyrie" { Some("Angel") } else { None },
                "current_effective_entry_characteristics":true,"tokens_count":true,"trigger_count_per_entering_creature":1}),
            if d.name == "Dazzling Angel" {
                gain()
            } else if d.name == "Beast-Kin Ranger" {
                EffectOp::BindTemporaryBoostToTriggerSource {
                    power: 1,
                    toughness: 0,
                }
            } else {
                EffectOp::BindPlusOnePlusOneCounterToTriggerSource
            },
            if d.name == "Dazzling Angel" {
                vec![gain_meaning()]
            } else if d.name == "Beast-Kin Ranger" {
                vec![boost_meaning(1, 0)]
            } else {
                vec![counter_meaning()]
            },
        ),
        "Spitfire Lagac" | "Tatyova, Benthic Druid" | "Elfsworn Giant" => (
            TriggerCondition::ControlledLandEnters,
            json!({"event":"LandEntersUnderObserverControl","requires_land_play":false,
                "tokens_count":true,"current_effective_entry_characteristics":true,"trigger_count_per_entering_land":1}),
            if d.name == "Spitfire Lagac" {
                EffectOp::DealDamage {
                    target: TargetRef::Opponent,
                    amount: 1,
                }
            } else if d.name == "Elfsworn Giant" {
                EffectOp::CreateToken {
                    token_def: crate::card_def::card_id_by_name("Elf Warrior Token")
                        .ok_or(Error::UnsupportedPrintedRoot(id))?,
                    controller: PlayerRef::Controller,
                }
            } else {
                EffectOp::Sequence(vec![
                    gain(),
                    EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                ])
            },
            if d.name == "Spitfire Lagac" {
                vec![
                    json!({"operation":"DealDamage","recipient":"OpponentOfCapturedTriggerController","amount":1,
                    "source":"OriginalTriggerSource","damage_class":"Noncombat",
                    "normal_damage_replacements_and_prevention_apply":true}),
                ]
            } else if d.name == "Elfsworn Giant" {
                vec![
                    json!({"operation":"CreateToken","controller":"CapturedTriggerController","count":1,
                    "token_definition":super::baseline(crate::card_def::card_id_by_name("Elf Warrior Token").ok_or(Error::UnsupportedPrintedRoot(id))?)?,
                    "normal_token_replacements_and_entry_triggers_apply":true}),
                ]
            } else {
                vec![
                    gain_meaning(),
                    json!({"operation":"DrawCards","recipient":"CapturedTriggerController","count":1,
                    "does_not_require_successful_life_gain":true,"normal_draw_replacements_apply":true}),
                ]
            },
        ),
        _ => unreachable!(),
    };
    let [trigger] = crate::trigger::triggers_for(id) else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let crate::trigger::TriggeredAbilityDef {
        condition: actual,
        home_zone,
        intervening_if_kicked,
        intervening_if_controls_another_source_card,
        face_index,
        effect,
    } = trigger;
    if *actual != condition
        || *home_zone != Zone::Battlefield
        || *intervening_if_kicked
        || *intervening_if_controls_another_source_card
        || *face_index != 0
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(json!({"card_id":id,"observer_trigger_printing":{
        "schema":"printed-battlefield-observer/v1","cost":cost,"types":d.types,"subtypes":d.subtypes,
        "supertypes":if supertypes.is_empty() { json!([]) } else { json!(["Legendary"]) },"colors":d.colors,"power":power,"toughness":toughness,
        "keywords":keywords.0,"mana_value":d.mana_value,"token":false,
        "ordinary_cast":{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
            "timing":if keywords.has(Keywords::FLASH) {"InstantWithPriority"} else {"OwnMainPhaseEmptyStackWithPriority"},"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "targets":[],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
            "other_cast_permissions_require_current_candidate_authority":true,
            "countered_or_failed_spell_destination":"OwnersGraveyard","normal_zone_replacements_and_entry_triggers_apply":true},
        "ordered_triggers":[{"event":event,"home_zone":"Battlefield","source_face":0,"intervening_if":null,
            "mandatory":true,"targets":[],"uses_stack":true,"controller":"ObserverControllerAtEvent",
            "captures_source_incarnation_and_controller_at_event":true,"survives_source_departure_and_later_control_change":true,
            "ordered_resolution":ordered}]
    }})))
}
