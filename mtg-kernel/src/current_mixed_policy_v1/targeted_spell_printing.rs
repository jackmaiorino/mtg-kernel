//! Closed one-target spells. Native executable programs are compared locally,
//! never serialized into model inputs or used as executable authority.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, CostComponent, DynamicValueDef, Keywords, TargetSpec},
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
            | "Joust Through"
            | "Fleeting Distraction"
            | "Sure Strike"
            | "Snakeskin Veil"
            | "Hero's Downfall"
            | "Broken Wings"
            | "Make Your Move"
            | "Zombify"
            | "Bulk Up"
            | "Incinerating Blast"
            | "Burst Lightning"
            | "Stroke of Midnight"
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
    if d.name == "Bulk Up" {
        return bulk_up(id, d);
    }
    if d.name == "Incinerating Blast" {
        return incinerating_blast(id, d);
    }
    if d.name == "Burst Lightning" {
        return burst_lightning(id, d);
    }
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
        "Stroke of Midnight" => {
            let human = crate::card_def::card_id_by_name("Human Token")
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
            let human_meaning = super::baseline(human)?;
            (
                ManaColor::W,
                2,
                &[Pip::Colored(ManaColor::W)][..],
                CardType::Instant,
                TargetSpec::NonlandPermanent,
                json!({"type":"Permanent","must_not_have_effective_type":"Land","either_controller":true}),
                EffectOp::DestroyObjectThenCreateTokens {
                    object: ObjectRef::Target(0),
                    token_def: human,
                    count: 1,
                },
                vec![
                    json!({"operation":"DestroyPermanentThenCreateTokensForCapturedController",
                    "target_slot":0,"capture_target_current_controller_before_destruction":true,
                    "token_recipient_is_captured_controller_not_owner_or_spell_controller":true,
                    "requires_current_battlefield_incarnation":true,
                    "respects_indestructible_and_normal_zone_replacements":true,
                    "token_count":1,"token_card_id":human,"token_printing":human_meaning,
                    "create_even_if_legal_target_is_not_destroyed":true,
                    "tokens_enter_under_captured_controllers_control_and_ownership":true,
                    "normal_token_creation_entry_replacements_and_triggers_apply":true,
                    "no_priority_or_state_based_actions_between_destruction_and_creation":true}),
                ],
            )
        }
        "Joust Through" => (
            ManaColor::W,
            0,
            &[Pip::Colored(ManaColor::W)][..],
            CardType::Instant,
            TargetSpec::AttackingOrBlockingCreature,
            json!({"type":"Creature","current_combat_role_any_of":["Attacking","Blocking"]}),
            EffectOp::Sequence(vec![
                EffectOp::DealDamage {
                    target: TargetRef::Target(0),
                    amount: 3,
                },
                EffectOp::GainLife {
                    player: PlayerRef::Controller,
                    amount: 1,
                },
            ]),
            vec![
                json!({"operation":"DealDamage","target_slot":0,"amount":3,
                    "damage_class":"Noncombat","source":"ResolvingSpell",
                    "normal_damage_replacements_and_prevention_apply":true}),
                json!({"operation":"GainLife","recipient":"SpellController","amount":1,
                    "does_not_require_positive_damage_dealt":true,"normal_life_gain_replacements_apply":true}),
            ],
        ),
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

fn incinerating_blast(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let cost = Cost {
        generic: 4,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    let program = EffectOp::Sequence(vec![
        EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 6,
        },
        EffectOp::MayPayCostThen {
            discard: 1,
            sacrifice_lands: 0,
            return_permanent: None,
            then: Box::new(EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            }),
            otherwise: None,
        },
    ]);
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 5
        || d.types != [CardType::Sorcery]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::R]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::Creature
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(program)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let optional = json!({"operation":"MayDiscardThenDraw","chooser":"CapturedSpellController",
        "offer_only_if_entire_one_card_discard_is_payable":true,"may_decline":true,
        "ordered_costs":[{"operation":"DiscardOwnHandCards","count":1,"any_card_type_including_land":true,
            "selection_private_to_payer_until_discard":true,"normal_discard_and_zone_replacements_apply":true}],
        "if_paid":[{"operation":"DrawCards","recipient":"CapturedSpellController","count":1,
            "drawn_identity_visible_only_to_authorized_viewers":true,"normal_draw_replacements_and_empty_library_rules_apply":true}],
        "if_declined_or_unpayable":[],"discard_is_resolution_cost_not_cast_cost":true,
        "damage_prevention_or_target_survival_does_not_skip_optional_instruction":true});
    let resolution = json!([{"operation":"DealNoncombatDamage","source":"OriginalResolvingSpell","target_slot":0,"amount":6,
        "normal_source_keywords_replacements_and_prevention_apply":true},optional]);
    let casts = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
        "target_count":1,"target_slot":0,"target_filter":{"zone":"Battlefield","effective_type":"Creature","either_controller":true},
        "requires_current_target_incarnation":true,"recheck_target_legality_at_resolution":true,
        "all_targets_illegal_skips_damage_and_optional_discard_draw":true,"ordered_resolution":resolution,
        "no_response_priority_or_state_based_actions_between_damage_and_finished_optional_cost":true,
        "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
        "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone","stack_created_copies_do_not_pay_cast_costs":true,
        "countering_skips_damage_and_optional_discard_draw_without_refunding_mana":true,
        "normal_triggers_and_state_based_actions_follow_complete_resolution":true,
        "normal_zone_replacements_apply":true,
        "other_cast_permissions_require_current_candidate_authority":true}]);
    Ok(Some(
        json!({"card_id":id,"damage_optional_discard_draw_printing":{
            "schema":"printed-creature-damage-optional-discard-draw/v1","cost":cost,"mana_value":5,
            "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,"power":null,"toughness":null,"keywords":0,"token":false,
            "ordered_cast_roots":casts
        }}),
    ))
}

fn burst_lightning(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let cost = Cost {
        generic: 0,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    let kicker = Cost {
        generic: 4,
        pips: &[],
        x_count: 0,
    };
    let damage = |amount| EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount,
    };
    let program = EffectOp::Conditional {
        cond: EffectCond::WasKicked,
        then: Box::new(damage(4)),
        else_: Box::new(damage(2)),
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.kicker_cost != Some(kicker)
        || d.mana_value != 1
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
        || d.target_spec != TargetSpec::AnyTarget
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(program)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let roots = [false, true].into_iter().map(|kicked| {
        let total = Cost { generic: if kicked { 4 } else { 0 }, pips: cost.pips, x_count: 0 };
        let costs = json!([{"operation":"PayTotalManaCost","printed_mana_component":cost,
            "chosen_kicker_additional_component":if kicked { Some(kicker) } else { None },
            "base_total_before_increases_and_reductions":total,"one_native_payment_plan":true}]);
        json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
            "timing":"InstantWithPriority","kicked":kicked,"kicker_chosen_during_cast_before_payment":true,
            "ordered_costs":costs,"normal_cost_increases_and_reductions_apply_to_total_cost":true,
            "target_count":1,"target_slot":0,"target_filter":{"kind":"AnyTarget","players":true,
                "battlefield_effective_types":["Creature","Planeswalker"],"either_controller":true},
            "requires_current_target_incarnation":true,"recheck_target_legality_at_resolution":true,
            "all_targets_illegal_skips_entire_resolution":true,
            "ordered_resolution":[{"operation":"DealNoncombatDamage","source":"OriginalResolvingSpell",
                "target_slot":0,"amount":if kicked {4}else{2},"damage_occurrences":1,
                "normal_source_keywords_replacements_and_prevention_apply":true}],
            "kicked_damage_replaces_base_damage_not_an_extra_packet":true,
            "stack_created_copies_preserve_original_kicked_choice_without_paying_again":true,
            "countering_skips_resolution_without_refunding_cost":true,
            "other_cast_permissions_require_current_candidate_authority":true})
    }).collect::<Vec<_>>();
    Ok(Some(json!({"card_id":id,"kicker_damage_printing":{
        "schema":"printed-kicker-any-target-damage/v1","cost":cost,"kicker_additional_cost":kicker,
        "mana_value":1,"types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"token":false,"ordered_cast_roots":roots,
        "kicker_does_not_change_printed_or_stack_mana_value":true,
        "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
        "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone","normal_zone_replacements_apply":true
    }})))
}

fn bulk_up(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let normal = Cost {
        generic: 1,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    let flashback = Cost {
        generic: 4,
        pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != normal
        || d.mana_value != 2
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
        || d.target_spec != TargetSpec::Creature
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(EffectOp::DoubleTargetPowerUntilEndOfTurn { target_index: 0 })
        || !d
            .flashback
            .as_ref()
            .is_some_and(|f| f.cost == [CostComponent::Mana(flashback)])
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let resolution = json!([{"operation":"DoubleCurrentEffectiveCreaturePower","target_slot":0,
        "sample_time":"Resolution","sample_once":true,"sample_is_full_native_signed_power":true,
        "implementation":"AddSampledPowerAsSeparateAdditivePowerModifier","negative_power_remains_negative":true,
        "zero_power_adds_no_modifier":true,"toughness_delta":0,"places_no_counters":true,
        "binding":"OriginalTargetBattlefieldIncarnation","layer":"AdditivePowerToughness",
        "later_characteristic_changes_do_not_resample_modifier":true,
        "multiple_resolutions_sample_then_current_power_independently":true,
        "duration":"UntilCleanupExpirationAfterDiscard"}]);
    let roots=[false,true].into_iter().map(|fb| json!({
        "method":if fb{"Flashback"}else{"Normal"},"route":if fb{"GraveyardFlashback"}else{"Hand"},
        "origin":if fb{"OwnGraveyard"}else{"OwnHand"},"origin_owner_must_be_caster":true,
        "timing":"InstantWithPriority","ordered_costs":[{"operation":if fb{"PayFlashbackAlternativeMana"}else{"PayPrintedMana"},
            "cost":if fb{flashback}else{normal},"normal_cost_increases_and_reductions_apply":true}],
        "target_count":1,"target_slot":0,"target_filter":{"zone":"Battlefield","effective_type":"Creature","either_controller":true},
        "requires_current_target_incarnation":true,"recheck_target_legality_at_resolution":true,
        "all_targets_illegal_skips_entire_resolution":true,"ordered_resolution":resolution,
        "physical_stack_departure":if fb{"ExileInsteadOfAnyOtherDeparture"}else{"GraveyardAfterResolutionOrCounter"},
        "countering_skips_resolution_without_refunding_cost":true,"other_cast_permissions_require_current_candidate_authority":true
    })).collect::<Vec<_>>();
    Ok(Some(json!({"card_id":id,"bulk_up_printing":{
        "schema":"printed-double-power-flashback/v1","cost":normal,"mana_value":2,"types":d.types,
        "subtypes":[],"supertypes":[],"colors":d.colors,"power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":roots,"flashback_cost_is_alternative_not_additional":true,
        "flashback_does_not_change_printed_or_stack_mana_value":true,"virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
        "normal_zone_replacements_apply":true
    }})))
}
