//! Closed one-target spells. Native executable programs are compared locally,
//! never serialized into model inputs or used as executable authority.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, DynamicValueDef, Keywords, TargetSpec},
    effect::{EffectCond, EffectOp, ObjectRef, PlayerRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Stab"
            | "Fleeting Flight"
            | "Bake into a Pie"
            | "Fleeting Distraction"
            | "Sure Strike"
            | "Snakeskin Veil"
            | "Hero's Downfall"
            | "Broken Wings"
            | "Make Your Move"
            | "Zombify"
    )
}
fn pump(power: i32, toughness: i32) -> EffectOp {
    EffectOp::PumpTargetUntilEndOfTurnDynamic {
        target: TargetRef::Target(0),
        power: DynamicValueDef::Fixed(power),
        toughness: DynamicValueDef::Fixed(toughness),
    }
}
fn pump_meaning(power: i32, toughness: i32) -> Value {
    json!({"operation":"ModifyPowerToughness","target_slot":0,
        "power_delta":power,"toughness_delta":toughness,
        "binding":"OriginalTargetIncarnation","layer":"AdditivePowerToughness",
        "duration":"UntilCleanupExpirationAfterDiscard"})
}
fn grant_meaning(keyword: Keywords) -> Value {
    json!({"operation":"GrantKeyword","target_slot":0,"keyword_bits":keyword.0,
        "binding":"OriginalTargetIncarnation","duration":"UntilCleanupExpirationAfterDiscard"})
}
/// All other CardDef fields and trigger absence are checked by the caller's
/// shared exclusion guard before this printing can be published.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (color, generic, pips, kind, target, filter, program, ordered) = match d.name {
        "Stab" => (
            ManaColor::B,
            0,
            &[Pip::Colored(ManaColor::B)][..],
            CardType::Instant,
            TargetSpec::Creature,
            json!({"type":"Creature"}),
            pump(-2, -2),
            vec![pump_meaning(-2, -2)],
        ),
        "Fleeting Flight" => (
            ManaColor::W,
            0,
            &[Pip::Colored(ManaColor::W)][..],
            CardType::Instant,
            TargetSpec::Creature,
            json!({"type":"Creature"}),
            EffectOp::Sequence(vec![
                EffectOp::AddCountersToTarget {
                    target_index: 0,
                    optional: false,
                    plus1_plus1: 1,
                    lifelink: 0,
                    stun: 0,
                },
                EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::Target(0),
                    keyword: Keywords::FLYING,
                },
                EffectOp::PreventCombatDamageToTargetThisTurn { target_index: 0 },
            ]),
            vec![
                json!({"operation":"AddCounters","target_slot":0,"counter":"PlusOnePlusOne","count":1,
                    "mandatory":true,"permanent_counters":true,"normal_counter_replacements_apply":true}),
                grant_meaning(Keywords::FLYING),
                json!({"operation":"PreventCombatDamageToCreature","target_slot":0,
                    "binding":"OriginalTargetIncarnation","duration":"UntilCleanupExpirationAfterDiscard",
                    "damage_class":"CombatOnly","prevents_damage_to_target_not_damage_dealt_by_target":true,
                    "normal_damage_prevention_replacement_order_applies":true}),
            ],
        ),
        "Bake into a Pie" => {
            let food = crate::card_def::card_id_by_name("Food Token")
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
            let food_meaning = super::super::next_five::printing(food)?
                .ok_or(Error::UnsupportedPrintedRoot(food))?;
            (
                ManaColor::B,
                2,
                &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)][..],
                CardType::Instant,
                TargetSpec::Creature,
                json!({"type":"Creature"}),
                EffectOp::Sequence(vec![
                    EffectOp::Conditional {
                        cond: EffectCond::TargetInZone(0, Zone::Battlefield),
                        then: Box::new(EffectOp::DestroyObject {
                            object: ObjectRef::Target(0),
                        }),
                        else_: Box::new(EffectOp::Sequence(vec![])),
                    },
                    EffectOp::CreateToken {
                        token_def: food,
                        controller: PlayerRef::Controller,
                    },
                ]),
                vec![
                    json!({"operation":"DestroyPermanent","target_slot":0,
                        "requires_current_battlefield_incarnation":true,
                        "respects_indestructible_and_normal_replacements":true}),
                    json!({"operation":"CreateToken","recipient":"SpellController","count":1,
                        "token_card_id":food,"token_printing":food_meaning,
                        "does_not_require_successful_destruction":true,
                        "normal_entry_replacements_and_triggers_apply":true}),
                ],
            )
        }
        "Fleeting Distraction" => (
            ManaColor::U,
            0,
            &[Pip::Colored(ManaColor::U)][..],
            CardType::Instant,
            TargetSpec::Creature,
            json!({"type":"Creature"}),
            EffectOp::Sequence(vec![
                pump(-1, 0),
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
            ]),
            vec![
                pump_meaning(-1, 0),
                json!({"operation":"DrawCards","recipient":"SpellController","count":1}),
            ],
        ),
        "Sure Strike" => (
            ManaColor::R,
            1,
            &[Pip::Colored(ManaColor::R)][..],
            CardType::Instant,
            TargetSpec::Creature,
            json!({"type":"Creature"}),
            EffectOp::Sequence(vec![
                pump(3, 0),
                EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::Target(0),
                    keyword: Keywords::FIRST_STRIKE,
                },
            ]),
            vec![pump_meaning(3, 0), grant_meaning(Keywords::FIRST_STRIKE)],
        ),
        "Snakeskin Veil" => (
            ManaColor::G,
            0,
            &[Pip::Colored(ManaColor::G)][..],
            CardType::Instant,
            TargetSpec::ControlledCreature,
            json!({"type":"Creature","controller":"Caster"}),
            EffectOp::Sequence(vec![
                EffectOp::AddCountersToTarget {
                    target_index: 0,
                    optional: false,
                    plus1_plus1: 1,
                    lifelink: 0,
                    stun: 0,
                },
                EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::Target(0),
                    keyword: Keywords::HEXPROOF,
                },
            ]),
            vec![
                json!({"operation":"AddCounters","target_slot":0,"counter":"PlusOnePlusOne","count":1,
                "mandatory":true,"permanent_counters":true,"normal_counter_replacements_apply":true}),
                grant_meaning(Keywords::HEXPROOF),
            ],
        ),
        "Hero's Downfall" | "Broken Wings" | "Make Your Move" => {
            let (color, generic, pips, target, filter) = match d.name {
                "Hero's Downfall" => (
                    ManaColor::B,
                    1,
                    &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)][..],
                    TargetSpec::CreatureOrPlaneswalker,
                    json!({"any_of_types":["Creature","Planeswalker"]}),
                ),
                "Broken Wings" => (
                    ManaColor::G,
                    2,
                    &[Pip::Colored(ManaColor::G)][..],
                    TargetSpec::ArtifactEnchantmentOrFlyingCreature,
                    json!({"any_of":[{"type":"Artifact"},{"type":"Enchantment"},{"type":"Creature","required_keyword_bits":Keywords::FLYING.0}]}),
                ),
                _ => (
                    ManaColor::W,
                    2,
                    &[Pip::Colored(ManaColor::W)][..],
                    TargetSpec::ArtifactEnchantmentOrCreaturePowerAtLeastFour,
                    json!({"any_of":[{"type":"Artifact"},{"type":"Enchantment"},{"type":"Creature","minimum_current_power":4}]}),
                ),
            };
            (
                color,
                generic,
                pips,
                CardType::Instant,
                target,
                filter,
                EffectOp::Conditional {
                    cond: EffectCond::TargetInZone(0, Zone::Battlefield),
                    then: Box::new(EffectOp::DestroyObject {
                        object: ObjectRef::Target(0),
                    }),
                    else_: Box::new(EffectOp::Sequence(vec![])),
                },
                vec![json!({"operation":"DestroyPermanent","target_slot":0,
                    "requires_current_battlefield_incarnation":true,
                    "respects_indestructible_and_normal_replacements":true})],
            )
        }
        "Zombify" => (
            ManaColor::B,
            3,
            &[Pip::Colored(ManaColor::B)][..],
            CardType::Sorcery,
            TargetSpec::CreatureCardInOwnGraveyard,
            json!({"type":"CreatureCard","owner":"Caster","graveyard_of":"Caster","nontoken":true}),
            EffectOp::MoveObject {
                object: ObjectRef::Target(0),
                to_zone: Zone::Battlefield,
            },
            vec![
                json!({"operation":"MoveCreatureCard","target_slot":0,"from":"CastersGraveyard","to":"Battlefield",
                "entry_controller":"Caster","new_battlefield_incarnation":true,"normal_entry_replacements_and_triggers_apply":true}),
            ],
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
        || d.colors != [color]
        || usize::from(d.mana_value) != usize::from(generic) + pips.len()
        || d.is_land
        || d.is_token
        || d.types != [kind]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.target_spec != target
        || d.mode2.is_some()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(program)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let graveyard = d.name == "Zombify";
    Ok(Some(json!({"card_id":id,"primitive_printing":{
        "cost":cost,"types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"mana_value":d.mana_value,"token":false,
        "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand",
            "origin_owner_must_be_caster":true,
            "timing":if kind==CardType::Sorcery {"OwnMainPhaseEmptyStackWithPriority"}else{"InstantWithPriority"},
            "ordered_costs":[{"PrintedMana":cost}],"target_count":1,"target_slot":0,
            "target_zone":if graveyard {"Graveyard"}else{"Battlefield"},"target_filter":filter,
            "target_characteristics":if graveyard {"PrintedCardTypes"}else{"CurrentEffectiveBattlefieldCharacteristics"},
            "requires_current_target_zone_incarnation":true,"recheck_target_filter_at_resolution":true,
            "all_targets_illegal_skips_entire_resolution":true,"ordered_resolution":ordered,
            "physical_stack_departure":"GraveyardAfterResolutionOrCounter","virtual_spell_copy_ceases":true,
            "other_cast_permissions_require_current_candidate_authority":true}]
    }})))
}
