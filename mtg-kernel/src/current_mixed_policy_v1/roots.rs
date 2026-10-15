//! Exact small baseline printing descriptors. Other missing typed roots refuse.
use super::{Error, Result};
use crate::card_def::{CardCapability, CardDef, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{EffectCond, EffectOp, ObjectRef, PlayerRef};
use crate::state::Zone;
use serde_json::{json, Value};

/// A printing-owned route, not a cast permission or a quoted payment plan.
/// The current typed candidate/source zone and cast state select the route;
/// the owning backend still authenticates and pays it before execution.
#[derive(serde::Serialize)]
struct ThinkTwiceCastRoot {
    method: crate::state::CastMethodV4,
    route: crate::state::SpellCastRouteV4,
    origin: Zone,
    origin_owner_must_be_caster: bool,
    timing: BaselineTiming,
    ordered_costs: [BaselineManaCost; 1],
    ordered_resolution: [BaselineDraw; 1],
    physical_stack_departure: BaselineDeparture,
}
#[derive(serde::Serialize)]
enum BaselineTiming {
    InstantWithPriority,
}
#[derive(serde::Serialize)]
enum BaselineManaCost {
    PrintedMana(crate::mana::Cost),
    FlashbackAlternativeMana(crate::mana::Cost),
}
#[derive(serde::Serialize)]
enum BaselineDraw {
    DrawController(u32),
}
#[derive(serde::Serialize)]
enum BaselineDeparture {
    GraveyardAfterResolutionOrCounter,
    ExileInsteadOfAnyOtherDeparture,
}
const THINK_TWICE_NORMAL: crate::mana::Cost = crate::mana::Cost {
    pips: &[crate::mana::Pip::Colored(crate::mana::ManaColor::U)],
    generic: 1,
    x_count: 0,
};
const THINK_TWICE_FLASHBACK: crate::mana::Cost = crate::mana::Cost {
    generic: 2,
    ..THINK_TWICE_NORMAL
};
fn think_twice_cast_roots() -> [ThinkTwiceCastRoot; 2] {
    use crate::state::{CastMethodV4, SpellCastRouteV4};
    [
        ThinkTwiceCastRoot {
            method: CastMethodV4::Normal,
            route: SpellCastRouteV4::Hand,
            origin: Zone::Hand,
            origin_owner_must_be_caster: true,
            timing: BaselineTiming::InstantWithPriority,
            ordered_costs: [BaselineManaCost::PrintedMana(THINK_TWICE_NORMAL)],
            ordered_resolution: [BaselineDraw::DrawController(1)],
            physical_stack_departure: BaselineDeparture::GraveyardAfterResolutionOrCounter,
        },
        ThinkTwiceCastRoot {
            method: CastMethodV4::Flashback,
            route: SpellCastRouteV4::GraveyardFlashback,
            origin: Zone::Graveyard,
            origin_owner_must_be_caster: true,
            timing: BaselineTiming::InstantWithPriority,
            ordered_costs: [BaselineManaCost::FlashbackAlternativeMana(
                THINK_TWICE_FLASHBACK,
            )],
            ordered_resolution: [BaselineDraw::DrawController(1)],
            physical_stack_departure: BaselineDeparture::ExileInsteadOfAnyOtherDeparture,
        },
    ]
}

#[derive(serde::Serialize)]
enum CounterGuard {
    BoundSpellCanBeCountered,
    BoundTargetInStackLegacy,
}
#[derive(serde::Serialize)]
enum CounterTargetFilter {
    AnySpell,
    CreatureSpell,
    NoncreatureSpell,
}
#[derive(serde::Serialize)]
enum CounterInstruction {
    CounterBoundSpell {
        target_slot: u8,
        guard: CounterGuard,
        normal_physical_departure_proposal: Zone,
        flashback_replaces_departure_with: Zone,
        other_zone_replacements_still_apply: bool,
        virtual_copy_ceases: bool,
        failed_guard_is_noop: bool,
        already_paid_costs_are_not_refunded: bool,
    },
    DrawController(u32),
    DiscardController {
        requested_count: u32,
        clamp_to_available_hand: bool,
        private_choice_by_controller: bool,
        is_payment: bool,
    },
    CounterThenReward {
        target_slot: u8,
        counter: Box<CounterInstruction>,
        sample_target_spell_controller_before_counter: bool,
        reward_independent_of_counter_success: bool,
        token_count: u32,
        token_printing: Value,
    },
}
#[derive(serde::Serialize)]
struct CounterCastRoot {
    method: crate::state::CastMethodV4,
    route: crate::state::SpellCastRouteV4,
    origin: Zone,
    timing: BaselineTiming,
    ordered_costs: [BaselineManaCost; 1],
    target_count: u8,
    target_filter: CounterTargetFilter,
    target_may_be_own_or_opposing: bool,
    target_requires_exact_live_spell_stack_incarnation: bool,
    all_targets_illegal_skips_entire_resolution: bool,
    ordered_resolution: Vec<CounterInstruction>,
    physical_stack_departure: BaselineDeparture,
    virtual_spell_copy_ceases: bool,
}

/// Closed exact recipes only. Refute's loot and Offer's controller reward
/// remain distinct ordered operations, never approximations of a bare counter.
fn counter_cast_root(id: u16, d: &CardDef) -> Result<Option<CounterCastRoot>> {
    use crate::mana::{Cost, ManaColor, Pip};
    let (cost, target, filter) = match d.name {
        "Counterspell" => (
            Cost {
                pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)],
                generic: 0,
                x_count: 0,
            },
            TargetSpec::AnySpellOnStack,
            CounterTargetFilter::AnySpell,
        ),
        "Essence Scatter" => (
            THINK_TWICE_NORMAL,
            TargetSpec::CreatureSpellOnStack,
            CounterTargetFilter::CreatureSpell,
        ),
        "Refute" => (
            Cost {
                pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)],
                generic: 1,
                x_count: 0,
            },
            TargetSpec::AnySpellOnStack,
            CounterTargetFilter::AnySpell,
        ),
        "An Offer You Can't Refuse" => (
            Cost {
                pips: &[Pip::Colored(ManaColor::U)],
                generic: 0,
                x_count: 0,
            },
            TargetSpec::NoncreatureSpellOnStack,
            CounterTargetFilter::NoncreatureSpell,
        ),
        _ => return Ok(None),
    };
    if d.is_token
        || d.is_land
        || d.types != [CardType::Instant]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::U]
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(cost.generic) + cost.pips.len()
        || d.target_spec != target
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != crate::card_def::Keywords::NONE
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let condition = if cfg!(feature = "limited-fdn-fixtures") {
        EffectCond::TargetSpellCanBeCountered(0)
    } else {
        EffectCond::TargetInZone(0, Zone::Stack)
    };
    let counter = EffectOp::Conditional {
        cond: condition,
        then: Box::new(EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Graveyard,
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    };
    let typed_counter = || CounterInstruction::CounterBoundSpell {
        target_slot: 0,
        guard: if cfg!(feature = "limited-fdn-fixtures") {
            CounterGuard::BoundSpellCanBeCountered
        } else {
            CounterGuard::BoundTargetInStackLegacy
        },
        normal_physical_departure_proposal: Zone::Graveyard,
        flashback_replaces_departure_with: Zone::Exile,
        other_zone_replacements_still_apply: true,
        virtual_copy_ceases: true,
        failed_guard_is_noop: true,
        already_paid_costs_are_not_refunded: true,
    };
    let (program, instructions) = match d.name {
        "Refute" => (
            EffectOp::Sequence(vec![
                counter,
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                EffectOp::DiscardCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
            ]),
            vec![
                typed_counter(),
                CounterInstruction::DrawController(1),
                CounterInstruction::DiscardController {
                    requested_count: 1,
                    clamp_to_available_hand: true,
                    private_choice_by_controller: true,
                    is_payment: false,
                },
            ],
        ),
        "An Offer You Can't Refuse" => {
            let token_def = crate::card_def::card_id_by_name("Treasure Token")
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
            // This compound operation's counter always uses the protected-spell
            // guard, even in catalogs whose older simple counters only test zone.
            let mut counter = typed_counter();
            if let CounterInstruction::CounterBoundSpell { guard, .. } = &mut counter {
                *guard = CounterGuard::BoundSpellCanBeCountered;
            }
            (
                EffectOp::CounterTargetSpellThenCreateTokens {
                    target_index: 0,
                    token_def,
                    count: 2,
                },
                vec![CounterInstruction::CounterThenReward {
                    target_slot: 0,
                    counter: Box::new(counter),
                    sample_target_spell_controller_before_counter: true,
                    reward_independent_of_counter_success: true,
                    token_count: 2,
                    token_printing: baseline(token_def)?,
                }],
            )
        }
        _ => (counter, vec![typed_counter()]),
    };
    if (d.spell_effect)() != Some(program) {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(CounterCastRoot {
        method: crate::state::CastMethodV4::Normal,
        route: crate::state::SpellCastRouteV4::Hand,
        origin: Zone::Hand,
        timing: BaselineTiming::InstantWithPriority,
        ordered_costs: [BaselineManaCost::PrintedMana(cost)],
        target_count: 1,
        target_filter: filter,
        target_may_be_own_or_opposing: true,
        target_requires_exact_live_spell_stack_incarnation: true,
        all_targets_illegal_skips_entire_resolution: true,
        ordered_resolution: instructions,
        physical_stack_departure: BaselineDeparture::GraveyardAfterResolutionOrCounter,
        virtual_spell_copy_ceases: true,
    }))
}

pub(super) fn baseline(id: u16) -> Result<Value> {
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    // Every other field is checked by the common exclusion guard below. A
    // familiar name alone never admits a different cost, program or cast form.
    let think_twice = d.name == "Think Twice"
        && !d.is_token
        && !d.is_land
        && d.types == [CardType::Instant]
        && d.subtypes.is_empty()
        && d.supertypes.is_empty()
        && d.colors == [crate::mana::ManaColor::U]
        && d.cost == THINK_TWICE_NORMAL
        && d.mana_value == 2
        && d.power.is_none()
        && d.toughness.is_none()
        && d.keywords == crate::card_def::Keywords::NONE
        && d.target_spec == TargetSpec::None
        && d.produces_mana.is_empty()
        && d.mana_ability_choices.is_empty()
        && (d.mana_ability)().is_none()
        && d.flashback.as_ref().is_some_and(|flashback| {
            flashback.cost == [crate::card_def::CostComponent::Mana(THINK_TWICE_FLASHBACK)]
        })
        && (d.spell_effect)()
            == Some(EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            });
    let treasure = d.name == "Treasure Token"
        && d.is_token
        && d.types == [CardType::Artifact]
        && d.subtypes == [crate::card_def::Subtype::Treasure]
        && d.colors.is_empty()
        && d.supertypes.is_empty()
        && !d.is_land
        && d.mana_value == 0
        && d.produces_mana == d.mana_ability_choices
        && d.produces_mana
            == [
                crate::mana::ManaColor::W,
                crate::mana::ManaColor::U,
                crate::mana::ManaColor::B,
                crate::mana::ManaColor::R,
                crate::mana::ManaColor::G,
            ]
        && d.power.is_none()
        && d.toughness.is_none()
        && d.cost == crate::mana::Cost::zero()
        && d.target_spec == TargetSpec::None
        && d.keywords == crate::card_def::Keywords::NONE
        && (d.spell_effect)().is_none()
        && (d.mana_ability)().is_none()
        && d.mana_ability_def
            == Some(crate::card_def::ManaAbilityDef {
                cost: crate::card_def::ManaAbilityCostDef::TapAndSacrificeSelf,
                amount: crate::card_def::ManaAbilityAmountDef::Fixed(1),
                controller_damage: 0,
                max_activations_per_turn: None,
            });
    if !(d.capability == CardCapability::Full
        && d.object_name == d.name
        && d.activated_abilities.is_empty()
        && d.ward_cost.is_none()
        && d.alt_cost.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && (d.flashback.is_none() || think_twice)
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.optional_additional_cost.is_none()
        && d.equipment.is_none()
        && d.attachment.is_none()
        && d.starting_loyalty.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_creature_grant.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.generic_cost_reduction.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.kicked_target_spec.is_none()
        && d.escape.is_none()
        && !d.enters_battlefield_tapped
        && (d.mana_ability_def.is_none() || treasure)
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.enters_battlefield_tapped_unless.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && !d.changeling
        && d.bestow.is_none()
        && !d.delve
        && d.adventure.is_none()
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.additional_mana_ability_conditions.is_empty()
        && d.restricted_mana_abilities.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.token_counter_replacement_v48.is_none()
        && crate::trigger::triggers_for(id).is_empty())
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    if think_twice {
        return Ok(json!({"card_id":id,"primitive_printing":{
            "cost":d.cost,"types":d.types.iter().map(|v|*v as u8).collect::<Vec<_>>(),
            "subtypes":[],"supertypes":[],"colors":d.colors,"power":d.power,
            "toughness":d.toughness,"keywords":d.keywords.0,"mana_value":d.mana_value,
            "token":false,"ordered_cast_roots":think_twice_cast_roots(),
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
            "flashback_cost_is_alternative_not_additional":true
        }}));
    }
    if treasure {
        return Ok(
            json!({"card_id":id,"primitive_printing":{"token":true,"types":["Artifact"],"subtypes":["Treasure"],
            "ordered_mana_costs":["TapSelf","SacrificeSelf"],"mana_recipient":"Controller","mana_amount":1,"color_choices":d.mana_ability_choices}}),
        );
    }
    if let Some(root) = counter_cast_root(id, d)? {
        return Ok(json!({"card_id":id,"primitive_printing":{
            "cost":d.cost,"types":d.types.iter().map(|v|*v as u8).collect::<Vec<_>>(),
            "subtypes":[],"supertypes":[],"colors":d.colors,"power":d.power,
            "toughness":d.toughness,"keywords":d.keywords.0,"mana_value":d.mana_value,
            "token":false,"ordered_cast_roots":[root]
        }}));
    }
    let mana = match (d.mana_ability)() {
        None if d.produces_mana.is_empty() && d.mana_ability_choices.is_empty() => None,
        Some(EffectOp::Sequence(ops))
            if ops
                == vec![
                    EffectOp::TapObject {
                        object: ObjectRef::ThisSource,
                    },
                    EffectOp::AddMana {
                        player: PlayerRef::Controller,
                        colors: d.produces_mana.to_vec(),
                    },
                ]
                && d.produces_mana.len() == 1
                && d.mana_ability_choices == d.produces_mana =>
        {
            Some(d.produces_mana.to_vec())
        }
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    let instruction = match (d.spell_effect)() {
        None if mana.is_none()
            && (d.is_token && d.types == [CardType::Creature]
                || d.name == "Face-down creature"
                || d.name == "Face-down card") =>
        {
            json!({"operation":"PublicTokenOrMaskedCharacteristics","not_cast_permission":true})
        }
        None if d.is_land
            && d.types == [CardType::Land]
            && mana.is_some()
            && d.cost == crate::mana::Cost::zero()
            && d.target_spec == TargetSpec::None
            && d.power.is_none()
            && d.toughness.is_none() =>
        {
            json!({"operation":"PlayLand"})
        }
        Some(EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        }) if !d.is_land
            && d.types == [CardType::Creature]
            && d.target_spec == TargetSpec::None
            && d.power.is_some()
            && d.toughness.is_some()
            && (mana.is_none() || d.name == "Llanowar Elves") =>
        {
            json!({"operation":"EnterBattlefield","controller":"Caster"})
        }
        Some(EffectOp::DealDamage {
            target: crate::effect::TargetRef::Target(0),
            amount: 3,
        }) if d.name == "Lightning Bolt"
            && d.types == [CardType::Instant]
            && d.target_spec == TargetSpec::AnyTarget
            && !d.is_land
            && mana.is_none()
            && d.power.is_none()
            && d.toughness.is_none()
            && d.keywords == crate::card_def::Keywords::NONE =>
        {
            json!({"operation":"Damage","target_slot":0,"amount":3,"recipient_union":["Player","Creature","Planeswalker"]})
        }
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    Ok(json!({"card_id":id,"primitive_printing":{
        "cost":d.cost,"types":d.types.iter().map(|v|*v as u8).collect::<Vec<_>>(),
        "subtypes":d.subtypes.iter().map(|v|*v as u16).collect::<Vec<_>>(),
        "supertypes":d.supertypes.iter().map(|v|*v as u8).collect::<Vec<_>>(),
        "colors":d.colors,"power":d.power,"toughness":d.toughness,
        "keywords":d.keywords.0,"mana_value":d.mana_value,"token":d.is_token,
        "ordered_instructions":[instruction],
        "mana_ability":mana.map(|colors|json!({"cost":"TapSelf","recipient":"Controller","colors":colors,"amount":1}))
    }}))
}

pub(super) fn expand_thin(record: &serde_json::Map<String, Value>) -> Result<Value> {
    let id = record
        .get("card_id")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .ok_or(Error::Range)?;
    let root = record.get("root");
    macro_rules! typed {
        ($module:ident,$selector:ident,$function:ident,$argument:expr) => {{
            let selector =
                crate::$module::$selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            let meaning = crate::$module::$function(&selector, id, $argument)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            serde_json::to_value(meaning).map_err(|e| Error::Projection(e.to_string()))
        }};
    }
    match CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?
        .name
    {
        "Sower of Chaos" => typed!(
            rules_meaning_sower_v1,
            live_selector_v1,
            meaning_v1,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Wildwood Scourge" => {
            let selector = crate::rules_meaning_wildwood_v1::live_selector_v1()
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            let meaning = crate::rules_meaning_wildwood_v1::meaning_v1(&selector, id)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            serde_json::to_value(meaning).map_err(|e| Error::Projection(e.to_string()))
        }
        "Frenzied Goblin" => typed!(
            rules_meaning_frenzied_goblin_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Authority of the Consuls" => typed!(
            rules_meaning_authority_v1,
            live_selector_v1,
            meaning_v1,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Involuntary Employment" => typed!(
            rules_meaning_employment_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Slumbering Cerberus" => typed!(
            rules_meaning_cerberus_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Mild-Mannered Librarian" => typed!(
            rules_meaning_librarian_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Temporal Manipulation" => typed!(
            rules_meaning_extra_turn_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Juggernaut" => typed!(
            rules_meaning_juggernaut_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Ashroot Animist" => typed!(
            rules_meaning_ashroot_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Banishing Light" => typed!(
            rules_meaning_banishing_v1,
            live_selector_v1,
            meaning_v1,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Stromkirk Bloodthief" => typed!(
            rules_meaning_stromkirk_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Vampire Gourmand" => typed!(
            rules_meaning_gourmand_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Garruk's Uprising" => typed!(
            rules_meaning_garruk_v1,
            live_selector_v1,
            meaning_v1,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Crystal Barricade" => typed!(
            rules_meaning_crystal_barricade_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Arahbo, the First Fang" => typed!(
            rules_meaning_arahbo_v1,
            live_selector_v1,
            meaning_v1,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Strongbox Raider" => typed!(
            rules_meaning_strongbox_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Krenko, Mob Boss" => typed!(
            rules_meaning_krenko_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Zul Ashur, Lich Lord" => typed!(
            rules_meaning_zul_ashur_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Heraldic Banner" => typed!(
            rules_meaning_heraldic_banner_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Fishing Pole" => typed!(
            rules_meaning_fishing_pole_v39,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "High-Society Hunter" => typed!(
            rules_meaning_high_society_hunter_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Akroma's Memorial" => typed!(
            rules_meaning_akroma_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        _ => Err(Error::UnsupportedPrintedRoot(id)),
    }
}
