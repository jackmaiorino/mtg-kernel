//! Complete printed meanings for selected FullPool-only ordinary cast routes.
use super::{Error, Result};
use crate::{
    card_def::{
        CardCapability, CardDef, CardType, CombatStaticRulesDef, CostComponent, Keywords,
        PermanentFilter, Subtype, TargetSpec, CARD_DEFS,
    },
    effect::{CreatureFilter, EffectCond, EffectOp, ObjectRef, PlayerRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    scrawling_crawler_public_v1::{TriggerV1, NAME as CRAWLER},
    state::Zone,
    trigger::{self, TriggerCondition},
};
use serde_json::{json, Value};

const SLAGSTORM: &str = "Slagstorm";

pub(super) fn printing(id: u16) -> Result<Option<Value>> {
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    printing_for(id, d)
}

fn printing_for(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let body = match d.name {
        SLAGSTORM => slagstorm(id, d)?,
        CRAWLER => crawler(id, d)?,
        "Arbiter of Woe" => arbiter(id, d)?,
        "Needletooth Pack" => needletooth(id, d)?,
        _ => return Ok(None),
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.object_name != d.name
        || !common_shape(d)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(json!({
        "card_id": id,
        "full_pool_ordinary_printing": {
            "schema": "full-pool-ordinary-printing/v1",
            "cost": d.cost, "mana_value": d.mana_value, "types": d.types,
            "subtypes": d.subtypes, "supertypes": [], "colors": d.colors,
            "power": d.power, "toughness": d.toughness, "keywords": d.keywords.0,
            "cast_route": {
                "method": "Normal", "origin": "OwnHand", "timing": "SorceryWithPriority",
                "pay_printed_mana": true, "target_count": 0,
                "other_cast_permissions_require_current_candidate_authority": true,
                "countering_skips_resolution": true, "countering_does_not_refund_cost": true,
                "normal_replacements_and_state_based_actions_apply": true,
            },
            "printed_rules": body,
        },
    })))
}

fn common_shape(d: &CardDef) -> bool {
    // Exhaustive so a new definition field must be assigned a meaning before admission.
    matches!(
        d,
        CardDef {
            name: _,
            capability: CardCapability::Full,
            cost: _,
            generic_cost_reduction: None,
            ward_cost: None,
            spell_cannot_be_countered: false,
            records_mana_spent: false,
            equipment: None,
            types: _,
            subtypes: _,
            supertypes: [],
            power: _,
            toughness: _,
            starting_loyalty: None,
            enters_with_plus_one_counters: None,
            controlled_counter_keyword: None,
            game_outcome_protection_v1: None,
            controlled_creature_grant: None,
            is_land: false,
            produces_mana: [],
            colors: _,
            target_spec: TargetSpec::None,
            keywords: _,
            spell_effect: _,
            mana_ability: _,
            alt_cost: None,
            kicker_cost: None,
            additional_cost: _,
            flashback: None,
            activated_abilities: [],
            plot_cost: None,
            madness_cost: None,
            mode2: _,
            mode3: None,
            additional_modes: [],
            kicked_target_spec: None,
            is_token: false,
            escape: None,
            mana_ability_choices: [],
            enters_battlefield_tapped: false,
            mana_ability_def: None,
            minimum_blockers: 1,
            omen: None,
            mana_value: _,
            mana_ability_includes_chosen_color: false,
            as_enters_choose_color_other_than: None,
            additional_mana_abilities: [],
            object_name: _,
            enters_battlefield_tapped_unless: None,
            attachment: None,
            transform_face: None,
            saga: None,
            optional_additional_cost: None,
            changeling: false,
            bestow: None,
            delve: false,
            adventure: None,
            cant_be_blocked_by_monarchs_creatures: false,
            conditional_tap_yield: None,
            enters_tapped_unless_controller: None,
            additional_mana_ability_conditions: [],
            restricted_mana_abilities: [],
            animation: None,
            activated_ability_generic_reductions: [],
            combat_static_rules: CombatStaticRulesDef {
                must_attack_each_combat: false,
                cant_be_blocked_by_subtype: None
            },
            as_enters_color_choice_v1: None,
            chosen_color_creature_boost_v1: None,
            damage_replacement_v1: None,
            token_counter_replacement_v48: None,
            starting_life_self_bonuses_v1: [],
            source_type_hexproof_v1: None,
            other_controlled_nontoken_death_v1: None,
            creature_type_entry_v1: None,
        }
    ) && (d.mana_ability)().is_none()
}

fn slagstorm(id: u16, d: &CardDef) -> Result<Value> {
    let cost = Cost {
        generic: 1,
        pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    if d.cost != cost
        || d.mana_value != 3
        || d.keywords != Keywords::NONE
        || d.additional_cost.is_some()
        || d.types != [CardType::Sorcery]
        || !d.subtypes.is_empty()
        || d.colors != [ManaColor::R]
        || d.power.is_some()
        || d.toughness.is_some()
        || !crate::trigger::triggers_for(id).is_empty()
        || (d.spell_effect)()
            != Some(EffectOp::DamageAllCreatures {
                filter: CreatureFilter::All,
                amount: 3,
            })
        || !d.mode2.as_ref().is_some_and(|mode| {
            mode.target_spec == TargetSpec::None
                && (mode.effect)()
                    == EffectOp::Sequence(vec![
                        EffectOp::DealDamage {
                            target: TargetRef::Controller,
                            amount: 3,
                        },
                        EffectOp::DealDamage {
                            target: TargetRef::Opponent,
                            amount: 3,
                        },
                    ])
        })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(json!({
        "choose_exactly_one_mode_during_cast": true,
        "ordered_modes": [
            {"mode_index": 0, "operation": "DealNoncombatDamageToEachCreature",
             "amount": 3, "scope": "BothBattlefields", "timing": "SimultaneousBatch"},
            {"mode_index": 1, "operation": "DealNoncombatDamageToEachPlayer",
             "amount": 3, "native_recipient_order": ["Controller", "Opponent"]},
        ],
        "damage_source": "OriginalResolvingSpell",
        "physical_resolution_destination": "OwnersGraveyard",
        "spell_copy_departure": "CeaseWithoutEnteringCardZone",
    }))
}

fn crawler(id: u16, d: &CardDef) -> Result<Value> {
    let cost = Cost {
        generic: 3,
        pips: &[],
        x_count: 0,
    };
    if d.cost != cost
        || d.mana_value != 3
        || d.keywords != Keywords::NONE
        || d.additional_cost.is_some()
        || d.types != [CardType::Artifact, CardType::Creature]
        || d.subtypes != [Subtype::Phyrexian, Subtype::Construct]
        || !d.colors.is_empty()
        || d.power != Some(3)
        || d.toughness != Some(2)
        || d.mode2.is_some()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !crate::scrawling_crawler_public_v1::exact_triggers(id)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(json!({
        "resolution_destination": "BattlefieldUnderSpellController",
        "ordered_triggers": [
            {"trigger": TriggerV1::ControllerUpkeep,
             "when": "BeginningOfControllersUpkeep",
             "ordered_resolution": [{"operation": "Draw", "player": "Controller", "count": 1},
                                    {"operation": "Draw", "player": "Opponent", "count": 1}]},
            {"trigger": TriggerV1::OpponentDraw,
             "when": "EachSuccessfulOpponentDraw",
             "ordered_resolution": [{"operation": "LoseLife", "player": "Opponent", "amount": 1}]},
        ],
        "triggers_mandatory_untargeted": true,
        "trigger_controller_frozen_on_collection": true,
        "active_printed_trigger_source": "FaceUpBattlefieldWithAbilities",
    }))
}

fn has_trigger(id: u16, condition: TriggerCondition, target: TargetSpec, effect: EffectOp) -> bool {
    let [ability] = trigger::triggers_for(id) else {
        return false;
    };
    ability.condition == condition
        && ability.home_zone == Zone::Battlefield
        && ability.face_index == 0
        && !ability.intervening_if_kicked
        && !ability.intervening_if_controls_another_source_card
        && (ability.effect)() == effect
        && trigger::target_spec_for_trigger(id, &effect) == Some(target)
}

fn enters_as_creature(d: &CardDef) -> bool {
    d.types == [CardType::Creature]
        && d.mode2.is_none()
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
}

fn arbiter(id: u16, d: &CardDef) -> Result<Value> {
    let effect = EffectOp::Sequence(vec![
        EffectOp::DiscardCards {
            player: PlayerRef::Opponent,
            count: 1,
        },
        EffectOp::LoseLife {
            player: PlayerRef::Opponent,
            amount: 2,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ]);
    if d.cost
        != (Cost {
            generic: 4,
            pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)],
            x_count: 0,
        })
        || d.mana_value != 6
        || d.subtypes != [Subtype::Demon]
        || d.colors != [ManaColor::B]
        || d.power != Some(5)
        || d.toughness != Some(4)
        || d.keywords != Keywords::FLYING
        || !matches!(
            d.additional_cost,
            Some([CostComponent::SacrificeControlled {
                count: 1,
                filter: PermanentFilter::Creature
            }])
        )
        || !enters_as_creature(d)
        || !has_trigger(id, TriggerCondition::Etb, TargetSpec::None, effect)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(json!({
        "resolution_destination": "BattlefieldUnderSpellController",
        "mandatory_additional_cast_cost": {
            "operation": "Sacrifice", "controller": "Caster", "permanent_type": "Creature", "count": 1,
            "choose_and_pay_before_publication": true, "remains_required_with_other_cast_permissions": true,
        },
        "trigger": {
            "when": "ThisPermanentEntersBattlefield", "mandatory": true, "target_count": 0,
            "ordered_resolution": [
                {"operation": "Discard", "player": "Opponent", "count": 1,
                 "chooser": "DiscardingPlayer", "up_to_hand_size": true},
                {"operation": "LoseLife", "player": "Opponent", "amount": 2},
                {"operation": "Draw", "player": "Controller", "count": 1},
                {"operation": "GainLife", "player": "Controller", "amount": 2},
            ],
            "continues_after_discard_choice_or_empty_hand": true,
            "original_trigger_controller_retained": true,
        },
    }))
}

fn needletooth(id: u16, d: &CardDef) -> Result<Value> {
    let effect = EffectOp::Conditional {
        cond: EffectCond::CreatureDiedThisTurn,
        then: Box::new(EffectOp::AddPlusOnePlusOneCounters {
            object: ObjectRef::Target(0),
            count: 2,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    };
    if d.cost
        != (Cost {
            generic: 3,
            pips: &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)],
            x_count: 0,
        })
        || d.mana_value != 5
        || d.subtypes != [Subtype::Dinosaur]
        || d.colors != [ManaColor::G]
        || d.power != Some(4)
        || d.toughness != Some(5)
        || d.keywords != Keywords::NONE
        || d.additional_cost.is_some()
        || !enters_as_creature(d)
        || !has_trigger(
            id,
            TriggerCondition::BeginningControllerEndStepIfCreatureDied,
            TargetSpec::ControlledCreature,
            effect,
        )
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(json!({
        "resolution_destination": "BattlefieldUnderSpellController",
        "trigger": {
            "when": "BeginningOfControllersEndStep",
            "intervening_if": "AnyCreatureDiedThisTurn", "check_at_collection_and_resolution": true,
            "mandatory": true, "target_count": 1, "target_slot": 0,
            "target_kind": "ControlledCreature", "source_itself_eligible": true,
            "recheck_original_incarnation_and_target_legality_at_resolution": true,
            "ordered_resolution": [
                {"operation": "AddPlusOnePlusOneCounters", "target_slot": 0, "count": 2},
            ],
        },
    }))
}
