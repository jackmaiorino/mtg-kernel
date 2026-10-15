//! Exact small baseline printing descriptors. Other missing typed roots refuse.
use super::{Error, Result};
use crate::card_def::{CardCapability, CardDef, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{EffectCond, EffectOp, ObjectRef, PlayerRef};
use crate::state::Zone;
use serde_json::{json, Value};

#[path = "crystal_printing.rs"]
pub(super) mod crystal_printing;
#[path = "early_activation_printing.rs"]
pub(super) mod early_activation_printing;
#[path = "enchantment_hunter_printing.rs"]
pub(super) mod enchantment_hunter_printing;
#[path = "legendary_printing.rs"]
mod legendary_printing;
#[path = "rallier_bloom_printing.rs"]
pub(super) mod rallier_bloom_printing;
#[path = "tyrant_herald_printing.rs"]
pub(super) mod tyrant_herald_printing;

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
// These variant names are serialized semantic labels and must remain stable.
#[allow(clippy::enum_variant_names)]
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
    if matches!(
        d.name,
        "Banishing Light" | "Garruk's Uprising" | "High-Society Hunter"
    ) {
        return enchantment_hunter_printing::baseline(id, d);
    }
    if matches!(d.name, "Squad Rallier" | "Bloom Tender") {
        return rallier_bloom_printing::baseline(id, d);
    }
    if matches!(
        d.name,
        "Scavenging Ooze" | "Heartfire Immolator" | "Soul-Shackled Zombie"
    ) {
        return early_activation_printing::baseline(id, d);
    }
    if d.name == "Crystal Barricade" {
        return crystal_printing::baseline(id, d);
    }
    if matches!(d.name, "Twinflame Tyrant" | "Herald of Eternal Dawn") {
        return tyrant_herald_printing::baseline(id, d);
    }
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
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.other_controlled_nontoken_death_v1.is_none()
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

#[derive(serde::Serialize)]
struct OrdinaryPermanentCastRoot {
    method: crate::state::CastMethodV4,
    ordinary_route: crate::state::SpellCastRouteV4,
    ordinary_origin: Zone,
    timing: PermanentTiming,
    targets: PermanentTargets,
    ordered_costs: [BaselineManaCost; 1],
    ordered_resolution: [PermanentEntry; 1],
    physical_departure: PermanentDeparture,
    other_cast_permissions_require_current_candidate_authority: bool,
    zone_replacements_still_apply: bool,
}
#[derive(serde::Serialize)]
enum PermanentTiming {
    CasterMainPhaseEmptyStackWithPriority,
    InstantWithPriorityIncludingOpponentsTurnAndNonemptyStack,
}
#[derive(serde::Serialize)]
enum PermanentTargets {
    None,
}
#[derive(serde::Serialize)]
enum PermanentEntry {
    MoveOriginalSpellFromStackToBattlefieldUnderCasterControl,
    ResolveExistingWildwoodCapturedXEntry,
}
#[derive(serde::Serialize)]
enum PermanentDeparture {
    BattlefieldOnResolutionOtherwiseGraveyard,
}
/// Supplement the actual typed static root with the exact ordinary cast route.
/// A descriptor never supplies an executable program or current cast permission.
pub(super) fn elenda_printing(id: u16) -> Result<Value> {
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    let meaning = crate::rules_meaning_elenda_v52::printed_meaning(
        id,
        crate::rules_meaning_elenda_v52::RootV52::CreatureSpellAndPrintedStatics,
    )
    .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    if d.other_controlled_nontoken_death_v1.is_some()
        || d.cost.x_count != 0
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(json!({"card_id":id,"elenda_printing":{
        "meaning":meaning,
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {
            method:crate::state::CastMethodV4::Normal,
            ordinary_route:crate::state::SpellCastRouteV4::Hand,
            ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,
            targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(d.cost)],
            ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,
            zone_replacements_still_apply:true,
        }]
    }}))
}

/// The static printing, distinct from live effective characteristics or cast permission.
/// Supertypes are empty in this exact subset; their historical discriminants are
/// retained explicitly rather than serializing an unsupported executable definition.
#[derive(serde::Serialize)]
struct PermanentCharacteristics {
    types: Vec<crate::card_def::CardType>,
    subtypes: Vec<crate::card_def::Subtype>,
    supertypes: Vec<u8>,
    colors: Vec<crate::mana::ManaColor>,
    mana_value: u16,
    power: Option<i16>,
    toughness: Option<i16>,
    ordered_keywords: Vec<PermanentKeyword>,
    minimum_blockers: u8,
    token: bool,
    land: bool,
    enters_tapped: bool,
    printed_mana_abilities: u8,
    printed_activated_abilities: u8,
}
#[derive(serde::Serialize)]
enum PermanentKeyword {
    Flying,
    Deathtouch,
    Trample,
    Defender,
    Flash,
}

/// Mandatory even when one or all family meanings already marked this ID
/// covered. Only actor-authorized referenced cards reach this dispatch.
/// A changed printing with either name must refuse rather than use its old
/// public family facts as permission to omit the new/unsupported rule.
pub(super) fn registered_permanent_printing(id: u16) -> Result<Option<Value>> {
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    let keywords = match d.name {
        "Banishing Light" | "Garruk's Uprising" | "High-Society Hunter" => {
            return enchantment_hunter_printing::printing(id, d)
        }
        "Crystal Barricade" => return crystal_printing::printing(id, d),
        "Twinflame Tyrant" | "Herald of Eternal Dawn" => {
            return tyrant_herald_printing::printing(id, d)
        }
        "Squad Rallier" | "Bloom Tender" => return rallier_bloom_printing::printing(id, d),
        "Scavenging Ooze" | "Heartfire Immolator" | "Soul-Shackled Zombie" => {
            return early_activation_printing::printing(id, d)
        }
        "Arahbo, the First Fang" | "Zul Ashur, Lich Lord" => {
            return legendary_printing::printing(id, d)
        }
        "Frenzied Goblin" | "Heraldic Banner" => return attack_or_banner_printing(id, d),
        "Sower of Chaos" => return sower_printing(id, d),
        "Wildwood Scourge" => return wildwood_printing(id, d),
        "Ashroot Animist"
        | "Stromkirk Bloodthief"
        | "Strongbox Raider"
        | "Vampire Gourmand"
        | "Akroma's Memorial"
        | "Fiendish Panda" => {
            validate_family_permanent_recipe(id, d)?;
            if d.name == "Ashroot Animist" {
                vec![PermanentKeyword::Trample]
            } else {
                Vec::new()
            }
        }
        "Bloodthirsty Conqueror" => {
            if !crate::bloodthirsty_conqueror_v49::owns_definition(id) {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            vec![PermanentKeyword::Flying, PermanentKeyword::Deathtouch]
        }
        "Doubling Season" => {
            if !crate::doubling_season_v48::owns_definition(id) {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            Vec::new()
        }
        _ => return Ok(None),
    };
    // Complete family selectors or the closed 72-field recipes above authenticate
    // this ordinary program and reject foreign casts, statics or activations.
    // Check the mapped leaf here as well: no raw EffectOp enters the packet.
    if d.target_spec != TargetSpec::None
        || !d.activated_abilities.is_empty()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let characteristics = PermanentCharacteristics {
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: d.supertypes.iter().map(|value| *value as u8).collect(),
        colors: d.colors.to_vec(),
        mana_value: d.mana_value,
        power: d.power,
        toughness: d.toughness,
        ordered_keywords: keywords,
        minimum_blockers: d.minimum_blockers,
        token: d.is_token,
        land: d.is_land,
        enters_tapped: d.enters_battlefield_tapped,
        printed_mana_abilities: 0,
        printed_activated_abilities: 0,
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {
            method:crate::state::CastMethodV4::Normal,
            ordinary_route:crate::state::SpellCastRouteV4::Hand,
            ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,
            targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(d.cost)],
            ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,
            zone_replacements_still_apply:true,
        }]
    }})))
}

/// Family records need an independently closed ordinary printing, even when
/// they already carry some ordinary or triggered facts. The exact
/// ordinary recipe is independently required for every authorized reference.
/// Keep the exhaustive field pattern so a newly added CardDef field cannot
/// silently acquire an unauthenticated default in this closed supplement.
pub(super) fn validate_family_permanent_recipe(id: u16, d: &CardDef) -> Result<()> {
    use crate::card_def::{Keywords, Subtype};
    use crate::mana::{Cost, ManaColor, Pip};
    if CARD_DEFS
        .get(usize::from(id))
        .is_none_or(|actual| actual.name != d.name)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let (cost, mana_value, types, subtypes, supertypes, colors, power, toughness, keywords, grant) =
        match d.name {
            "Ashroot Animist" => (
                Cost {
                    generic: 2,
                    pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::G)],
                    x_count: 0,
                },
                4,
                &[CardType::Creature][..],
                &[Subtype::Lizard, Subtype::Druid][..],
                &[][..],
                &[ManaColor::R, ManaColor::G][..],
                Some(4),
                Some(4),
                Keywords::TRAMPLE,
                None,
            ),
            "Stromkirk Bloodthief" => (
                Cost {
                    generic: 2,
                    pips: &[Pip::Colored(ManaColor::B)],
                    x_count: 0,
                },
                3,
                &[CardType::Creature][..],
                &[Subtype::Vampire, Subtype::Rogue][..],
                &[][..],
                &[ManaColor::B][..],
                Some(2),
                Some(2),
                Keywords::NONE,
                None,
            ),
            "Strongbox Raider" => (
                Cost {
                    generic: 2,
                    pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)],
                    x_count: 0,
                },
                4,
                &[CardType::Creature][..],
                &[Subtype::Orc, Subtype::Pirate][..],
                &[][..],
                &[ManaColor::R][..],
                Some(5),
                Some(2),
                Keywords::NONE,
                None,
            ),
            "Vampire Gourmand" => (
                Cost {
                    generic: 1,
                    pips: &[Pip::Colored(ManaColor::B)],
                    x_count: 0,
                },
                2,
                &[CardType::Creature][..],
                &[Subtype::Vampire][..],
                &[][..],
                &[ManaColor::B][..],
                Some(2),
                Some(2),
                Keywords::NONE,
                None,
            ),
            "Akroma's Memorial" => (
                Cost {
                    generic: 7,
                    pips: &[],
                    x_count: 0,
                },
                7,
                &[CardType::Artifact][..],
                &[][..],
                &[crate::card_def::Supertype::Legendary][..],
                &[][..],
                None,
                None,
                Keywords::NONE,
                Some(crate::controlled_creature_grants_v1::MEMORIAL),
            ),
            "Fiendish Panda" => (
                crate::fiendish_panda_v1::PRINTED_COST,
                4,
                &[CardType::Creature][..],
                &[Subtype::Bear, Subtype::Demon][..],
                &[][..],
                &[ManaColor::W, ManaColor::B][..],
                Some(3),
                Some(2),
                Keywords::NONE,
                None,
            ),
            _ => return Err(Error::UnsupportedPrintedRoot(id)),
        };
    let crate::card_def::CardDef {
        name: _,
        capability: _,
        cost: _,
        generic_cost_reduction: _,
        ward_cost: _,
        spell_cannot_be_countered: _,
        records_mana_spent: _,
        equipment: _,
        types: _,
        subtypes: _,
        supertypes: _,
        power: _,
        toughness: _,
        starting_loyalty: _,
        enters_with_plus_one_counters: _,
        controlled_counter_keyword: _,
        game_outcome_protection_v1: _,
        controlled_creature_grant: _,
        is_land: _,
        produces_mana: _,
        colors: _,
        target_spec: _,
        keywords: _,
        spell_effect: _,
        mana_ability: _,
        alt_cost: _,
        kicker_cost: _,
        additional_cost: _,
        flashback: _,
        activated_abilities: _,
        plot_cost: _,
        madness_cost: _,
        mode2: _,
        mode3: _,
        additional_modes: _,
        kicked_target_spec: _,
        is_token: _,
        escape: _,
        mana_ability_choices: _,
        enters_battlefield_tapped: _,
        mana_ability_def: _,
        minimum_blockers: _,
        omen: _,
        mana_value: _,
        mana_ability_includes_chosen_color: _,
        as_enters_choose_color_other_than: _,
        additional_mana_abilities: _,
        object_name: _,
        enters_battlefield_tapped_unless: _,
        attachment: _,
        transform_face: _,
        saga: _,
        optional_additional_cost: _,
        changeling: _,
        bestow: _,
        delve: _,
        adventure: _,
        cant_be_blocked_by_monarchs_creatures: _,
        conditional_tap_yield: _,
        enters_tapped_unless_controller: _,
        additional_mana_ability_conditions: _,
        restricted_mana_abilities: _,
        animation: _,
        activated_ability_generic_reductions: _,
        combat_static_rules: _,
        as_enters_color_choice_v1: _,
        chosen_color_creature_boost_v1: _,
        damage_replacement_v1: _,
        token_counter_replacement_v48: _,
        starting_life_self_bonuses_v1: _,
        source_type_hexproof_v1: _,
        other_controlled_nontoken_death_v1: _,
    } = d;
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != cost
        || d.mana_value != mana_value
        || d.types != types
        || d.subtypes != subtypes
        || d.colors != colors
        || d.supertypes != supertypes
        || d.power != power
        || d.toughness != toughness
        || d.keywords != keywords
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || !d.activated_abilities.is_empty()
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.attachment.is_some()
        || d.starting_loyalty.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant != grant
        || d.game_outcome_protection_v1.is_some()
        || d.alt_cost.is_some()
        || d.kicker_cost.is_some()
        || d.additional_cost.is_some()
        || d.flashback.is_some()
        || d.plot_cost.is_some()
        || d.madness_cost.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.kicked_target_spec.is_some()
        || d.escape.is_some()
        || d.enters_battlefield_tapped
        || d.mana_ability_def.is_some()
        || d.omen.is_some()
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || !d.additional_mana_abilities.is_empty()
        || d.enters_battlefield_tapped_unless.is_some()
        || d.transform_face.is_some()
        || d.saga.is_some()
        || d.optional_additional_cost.is_some()
        || d.changeling
        || d.bestow.is_some()
        || d.delve
        || d.adventure.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.conditional_tap_yield.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.animation.is_some()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.as_enters_color_choice_v1.is_some()
        || d.chosen_color_creature_boost_v1.is_some()
        || d.damage_replacement_v1.is_some()
        || d.token_counter_replacement_v48.is_some()
        || !d.starting_life_self_bonuses_v1.is_empty()
        || d.source_type_hexproof_v1.is_some()
        || d.other_controlled_nontoken_death_v1.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    // Recompute the complete original typed trigger meaning from the actual
    // catalog/program, never from an observed name or supplied root digest.
    macro_rules! family {
        ($module:ident) => {{
            use crate::$module as family;
            let selector =
                family::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            family::meaning(&selector, id, 0).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            Ok(())
        }};
    }
    match d.name {
        "Ashroot Animist" => family!(rules_meaning_ashroot_v1),
        "Stromkirk Bloodthief" => family!(rules_meaning_stromkirk_v1),
        "Strongbox Raider" => family!(rules_meaning_strongbox_v1),
        "Vampire Gourmand" => family!(rules_meaning_gourmand_v1),
        "Akroma's Memorial" => {
            use crate::rules_meaning_akroma_v1 as family;
            let selector =
                family::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            for root in [
                family::RootV1::PermanentSpell,
                family::RootV1::ControlledCreatureGrant,
            ] {
                family::meaning(&selector, id, root)
                    .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            }
            Ok(())
        }
        "Fiendish Panda" => {
            use crate::rules_meaning_fiendish_panda_v1 as family;
            for root in [
                family::RootV1::CreatureSpell,
                family::RootV1::ControllerLifeGain,
                family::RootV1::SourceDeath,
            ] {
                let meaning = family::printed_meaning(id, root)
                    .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
                family::validate(&meaning).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            }
            Ok(())
        }
        _ => Err(Error::UnsupportedPrintedRoot(id)),
    }
}

/// These two older family meanings describe the attack/entry rules, not the
/// immutable ordinary printing. This supplement is mandatory even for covered
/// IDs, and admits exactly the closed native recipes below.
#[derive(serde::Serialize)]
struct BannerEntryReplacement {
    choices: [crate::mana::ManaColor; 5],
    chooser: EntryChooser,
    before_original_battlefield_move: bool,
    all_batch_choices_before_any_entry: bool,
    fresh_choice_each_entering_incarnation: bool,
}
#[derive(serde::Serialize)]
enum EntryChooser {
    ProposedBattlefieldController,
}
#[derive(serde::Serialize)]
struct BannerManaRoot {
    source_zone: Zone,
    ordered_costs: [PrintedManaAbilityCost; 1],
    recipient: PrintedManaRecipient,
    output: PrintedManaOutput,
    amount: u8,
    chosen_color_requires_authenticated_current_incarnation: bool,
    requires_active_effective_printed_ability: bool,
    creature_tap_timing_uses_current_effective_form_and_haste: bool,
    availability_requires_current_engine_mana_window: bool,
    spending_restriction: PrintedManaRestriction,
    uses_stack: bool,
}
#[derive(serde::Serialize)]
enum PrintedManaAbilityCost {
    TapOriginalSource,
}
#[derive(serde::Serialize)]
enum PrintedManaRecipient {
    CurrentSourceController,
}
#[derive(serde::Serialize)]
enum PrintedManaOutput {
    CurrentIncarnationChosenWubrgColor,
}
#[derive(serde::Serialize)]
enum PrintedManaRestriction {
    Unrestricted,
}
pub(super) fn attack_or_banner_printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    use crate::card_def::{
        AsEntersColorChoiceDefV1, ChosenColorCreatureBoostDefV1, Keywords, Subtype,
    };
    use crate::mana::{Cost, ManaColor, Pip};
    let banner = match d.name {
        "Frenzied Goblin" => false,
        "Heraldic Banner" => true,
        _ => return Ok(None),
    };
    let cost = if banner {
        Cost {
            generic: 3,
            pips: &[],
            x_count: 0,
        }
    } else {
        Cost {
            generic: 0,
            pips: &[Pip::Colored(ManaColor::R)],
            x_count: 0,
        }
    };
    // Every one of the current72 CardDef fields is admitted exactly or refused.
    // Name only chooses a recipe; it never authenticates a different program.
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != cost
        || d.mana_value != if banner { 3 } else { 1 }
        || d.types
            != if banner {
                &[CardType::Artifact][..]
            } else {
                &[CardType::Creature][..]
            }
        || d.subtypes
            != if banner {
                &[][..]
            } else {
                &[Subtype::Goblin, Subtype::Berserker][..]
            }
        || d.colors != if banner { &[][..] } else { &[ManaColor::R][..] }
        || !d.supertypes.is_empty()
        || d.power != if banner { None } else { Some(1) }
        || d.toughness != if banner { None } else { Some(1) }
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || !d.activated_abilities.is_empty()
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.attachment.is_some()
        || d.starting_loyalty.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.alt_cost.is_some()
        || d.kicker_cost.is_some()
        || d.additional_cost.is_some()
        || d.flashback.is_some()
        || d.plot_cost.is_some()
        || d.madness_cost.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.kicked_target_spec.is_some()
        || d.escape.is_some()
        || d.enters_battlefield_tapped
        || d.mana_ability_def.is_some()
        || d.omen.is_some()
        || d.mana_ability_includes_chosen_color != banner
        || d.as_enters_choose_color_other_than.is_some()
        || !d.additional_mana_abilities.is_empty()
        || d.enters_battlefield_tapped_unless.is_some()
        || d.transform_face.is_some()
        || d.saga.is_some()
        || d.optional_additional_cost.is_some()
        || d.changeling
        || d.bestow.is_some()
        || d.delve
        || d.adventure.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.conditional_tap_yield.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.animation.is_some()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.as_enters_color_choice_v1
            != if banner {
                Some(AsEntersColorChoiceDefV1::AnyWubrg)
            } else {
                None
            }
        || d.chosen_color_creature_boost_v1
            != if banner {
                Some(ChosenColorCreatureBoostDefV1 {
                    power: 1,
                    toughness: 0,
                })
            } else {
                None
            }
        || d.damage_replacement_v1.is_some()
        || d.token_counter_replacement_v48.is_some()
        || !d.starting_life_self_bonuses_v1.is_empty()
        || d.source_type_hexproof_v1.is_some()
        || d.other_controlled_nontoken_death_v1.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    if banner {
        use crate::rules_meaning_heraldic_banner_v1 as meaning;
        let selector = meaning::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        for root in [
            meaning::RootV1::PermanentSpell,
            meaning::RootV1::AsEntersColor,
            meaning::RootV1::ChosenColorAnthem,
            meaning::RootV1::ChosenColorMana,
        ] {
            meaning::meaning(&selector, id, root).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        }
        // Evaluate every supported choice through the actual native definition,
        // including its dynamically selected primary mana program.
        for color in crate::heraldic_banner_v1::COLORS {
            if d.primary_mana_ability_choices(Some(color)) != [color]
                || d.mana_ability_program_for(color, Some(color))
                    != Some(EffectOp::Sequence(vec![
                        EffectOp::TapObject {
                            object: ObjectRef::ThisSource,
                        },
                        EffectOp::AddMana {
                            player: PlayerRef::Controller,
                            colors: vec![color],
                        },
                    ]))
            {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
        }
    } else {
        use crate::rules_meaning_frenzied_goblin_v1 as meaning;
        let selector = meaning::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        meaning::meaning(&selector, id, 0).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    }
    let characteristics = PermanentCharacteristics {
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: Vec::new(),
        colors: d.colors.to_vec(),
        mana_value: d.mana_value,
        power: d.power,
        toughness: d.toughness,
        ordered_keywords: Vec::new(),
        minimum_blockers: d.minimum_blockers,
        token: false,
        land: false,
        enters_tapped: false,
        printed_mana_abilities: u8::from(banner),
        printed_activated_abilities: 0,
    };
    let entries = if banner {
        vec![BannerEntryReplacement {
            choices: crate::heraldic_banner_v1::COLORS,
            chooser: EntryChooser::ProposedBattlefieldController,
            before_original_battlefield_move: true,
            all_batch_choices_before_any_entry: true,
            fresh_choice_each_entering_incarnation: true,
        }]
    } else {
        Vec::new()
    };
    let mana = if banner {
        vec![BannerManaRoot {
            source_zone: Zone::Battlefield,
            ordered_costs: [PrintedManaAbilityCost::TapOriginalSource],
            recipient: PrintedManaRecipient::CurrentSourceController,
            output: PrintedManaOutput::CurrentIncarnationChosenWubrgColor,
            amount: 1,
            chosen_color_requires_authenticated_current_incarnation: true,
            requires_active_effective_printed_ability: true,
            creature_tap_timing_uses_current_effective_form_and_haste: true,
            availability_requires_current_engine_mana_window: true,
            spending_restriction: PrintedManaRestriction::Unrestricted,
            uses_stack: false,
        }]
    } else {
        Vec::new()
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {
            method:crate::state::CastMethodV4::Normal,
            ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(cost)],
            ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }],
        "ordered_entry_replacements":entries,"ordered_mana_roots":mana,
    }})))
}

/// Sower's family record owns the exact target/restriction meaning. The
/// mandatory ordinary supplement supplies its previously omitted printing and
/// native mana operands, without turning either record into action authority.
#[derive(serde::Serialize)]
struct SowerPrintedActivation {
    ability_index: u16,
    ordered_costs: [PrintedActivationCost; 1],
    timing: PrintedActivationTiming,
    original_source_incarnation_bound_before_payment: bool,
    payment_precedes_stack_publication: bool,
    countered_activation_never_refunds_payment: bool,
    meaning: crate::rules_meaning_sower_v1::MeaningV1,
}
#[derive(serde::Serialize)]
enum PrintedActivationCost {
    Mana(crate::mana::Cost),
}
#[derive(serde::Serialize)]
enum PrintedActivationTiming {
    WithPriorityAtInstantSpeed,
}
pub(super) fn sower_printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    use crate::card_def::{ActivationTargetFilter, CostComponent, Keywords, Subtype};
    use crate::mana::{Cost, ManaColor, Pip};
    if d.name != "Sower of Chaos" {
        return Ok(None);
    }
    let cost = Cost {
        generic: 3,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != cost
        || d.mana_value != 4
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Devil]
        || d.colors != [ManaColor::R]
        || !d.supertypes.is_empty()
        || d.power != Some(4)
        || d.toughness != Some(3)
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || d.activated_abilities.len() != 1
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.attachment.is_some()
        || d.starting_loyalty.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.alt_cost.is_some()
        || d.kicker_cost.is_some()
        || d.additional_cost.is_some()
        || d.flashback.is_some()
        || d.plot_cost.is_some()
        || d.madness_cost.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.kicked_target_spec.is_some()
        || d.escape.is_some()
        || d.enters_battlefield_tapped
        || d.mana_ability_def.is_some()
        || d.omen.is_some()
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || !d.additional_mana_abilities.is_empty()
        || d.enters_battlefield_tapped_unless.is_some()
        || d.transform_face.is_some()
        || d.saga.is_some()
        || d.optional_additional_cost.is_some()
        || d.changeling
        || d.bestow.is_some()
        || d.delve
        || d.adventure.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.conditional_tap_yield.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.animation.is_some()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.as_enters_color_choice_v1.is_some()
        || d.chosen_color_creature_boost_v1.is_some()
        || d.damage_replacement_v1.is_some()
        || d.token_counter_replacement_v48.is_some()
        || !d.starting_life_self_bonuses_v1.is_empty()
        || d.source_type_hexproof_v1.is_some()
        || d.other_controlled_nontoken_death_v1.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    if !crate::trigger::triggers_for(id).is_empty() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let ability = &d.activated_abilities[0];
    let activation_cost = Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    if ability.cost != [CostComponent::Mana(activation_cost)]
        || ability.target_spec != TargetSpec::Creature
        || (ability.effect)() != (EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 })
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    use crate::rules_meaning_sower_v1 as meaning;
    let selector = meaning::live_selector_v1().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    let meaning =
        meaning::meaning_v1(&selector, id, 0).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    let characteristics = PermanentCharacteristics {
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: Vec::new(),
        colors: d.colors.to_vec(),
        mana_value: d.mana_value,
        power: d.power,
        toughness: d.toughness,
        ordered_keywords: Vec::new(),
        minimum_blockers: d.minimum_blockers,
        token: false,
        land: false,
        enters_tapped: false,
        printed_mana_abilities: 0,
        printed_activated_abilities: 1,
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {
            method:crate::state::CastMethodV4::Normal,
            ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(cost)],
            ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }],
        "ordered_activation_roots":[SowerPrintedActivation {
            ability_index:0,ordered_costs:[PrintedActivationCost::Mana(activation_cost)],
            timing:PrintedActivationTiming::WithPriorityAtInstantSpeed,
            original_source_incarnation_bound_before_payment:true,payment_precedes_stack_publication:true,
            countered_activation_never_refunds_payment:true,meaning,
        }],
    }})))
}

/// A native variable-cost route, deliberately distinct from the fixed-cost
/// permanent template. Current source/option candidates own its actual quote.
#[derive(serde::Serialize)]
struct WildwoodVariableCastRoot {
    method: crate::state::CastMethodV4,
    ordinary_route: crate::state::SpellCastRouteV4,
    ordinary_origin: Zone,
    timing: PermanentTiming,
    targets: PermanentTargets,
    ordered_announcement: [NativeXSelection; 1],
    ordered_costs: [NativeVariableManaPayment; 1],
    ordered_resolution: [PermanentEntry; 1],
    mana_value: NativeVariableManaValue,
    physical_departure: PermanentDeparture,
    existing_entry_meaning_owns_the_only_counter_placement: bool,
    other_cast_permissions_require_current_candidate_authority: bool,
    zone_replacements_still_apply: bool,
}
#[derive(serde::Serialize)]
struct NativeXSelection {
    minimum: u16,
    supported_maximum: u16,
    menu: NativeXMenu,
    option_index_adds_to_minimum: bool,
    option_count_is_current_payable_maximum_minus_minimum_plus_one: bool,
    sole_payable_value_is_selected_automatically: bool,
    maximum_requires_current_native_payment_quote: bool,
    native_option_menu_requires_original_pending_cast: bool,
    native_without_mana_cost_permission_forces_zero_x: bool,
}
#[derive(serde::Serialize)]
enum NativeXMenu {
    SourceBoundChooseEffectOption,
}
#[derive(serde::Serialize)]
struct NativeVariableManaPayment {
    native_printed_cost: crate::mana::Cost,
    x_source: NativeXSource,
    each_printed_x_adds_chosen_x_generic_mana: bool,
    total_cost_modifiers_require_current_native_payment_quote: bool,
    payment_precedes_finalized_stack_publication: bool,
}
#[derive(serde::Serialize)]
enum NativeXSource {
    NativePendingCastChoiceThenFinalizedStackStateXValue,
}
#[derive(serde::Serialize)]
struct NativeVariableManaValue {
    printed_base: u16,
    stack_x_multiplicity: u8,
    stack_uses_finalized_captured_x: bool,
    pending_unfinalized_object_uses_printed_base: bool,
    off_stack_x_value: u16,
    maximum_represented_stack_value: u16,
}
pub(super) fn wildwood_printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    use crate::card_def::{Keywords, Subtype};
    use crate::mana::{Cost, ManaColor, Pip};
    if d.name != "Wildwood Scourge" {
        return Ok(None);
    }
    let cost = Cost {
        generic: 0,
        pips: &[Pip::Colored(ManaColor::G)],
        x_count: 1,
    };
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != cost
        || d.mana_value != 1
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Hydra]
        || d.colors != [ManaColor::G]
        || !d.supertypes.is_empty()
        || d.power != Some(0)
        || d.toughness != Some(0)
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)() != Some(EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters)
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || !d.activated_abilities.is_empty()
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.attachment.is_some()
        || d.starting_loyalty.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.alt_cost.is_some()
        || d.kicker_cost.is_some()
        || d.additional_cost.is_some()
        || d.flashback.is_some()
        || d.plot_cost.is_some()
        || d.madness_cost.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.kicked_target_spec.is_some()
        || d.escape.is_some()
        || d.enters_battlefield_tapped
        || d.mana_ability_def.is_some()
        || d.omen.is_some()
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || !d.additional_mana_abilities.is_empty()
        || d.enters_battlefield_tapped_unless.is_some()
        || d.transform_face.is_some()
        || d.saga.is_some()
        || d.optional_additional_cost.is_some()
        || d.changeling
        || d.bestow.is_some()
        || d.delve
        || d.adventure.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.conditional_tap_yield.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.animation.is_some()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.as_enters_color_choice_v1.is_some()
        || d.chosen_color_creature_boost_v1.is_some()
        || d.damage_replacement_v1.is_some()
        || d.token_counter_replacement_v48.is_some()
        || !d.starting_life_self_bonuses_v1.is_empty()
        || d.source_type_hexproof_v1.is_some()
        || d.other_controlled_nontoken_death_v1.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    // Authenticate both immutable native family roots. Their single existing
    // X-entry program owns counter placement; this descriptor adds no effect.
    use crate::rules_meaning_wildwood_v1 as meaning;
    let selector = meaning::live_selector_v1().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    let family =
        meaning::meaning_v1(&selector, id).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    let characteristics = PermanentCharacteristics {
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: Vec::new(),
        colors: d.colors.to_vec(),
        mana_value: d.mana_value,
        power: d.power,
        toughness: d.toughness,
        ordered_keywords: Vec::new(),
        minimum_blockers: d.minimum_blockers,
        token: false,
        land: false,
        enters_tapped: false,
        printed_mana_abilities: 0,
        printed_activated_abilities: 0,
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,
        "ordered_cast_roots":[WildwoodVariableCastRoot {
            method:crate::state::CastMethodV4::Normal,
            ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,targets:PermanentTargets::None,
            ordered_announcement:[NativeXSelection {
                minimum:family.entry.represented_x_minimum,supported_maximum:family.entry.represented_x_maximum,
                menu:NativeXMenu::SourceBoundChooseEffectOption,option_index_adds_to_minimum:true,
                option_count_is_current_payable_maximum_minus_minimum_plus_one:true,
                sole_payable_value_is_selected_automatically:true,maximum_requires_current_native_payment_quote:true,
                native_option_menu_requires_original_pending_cast:true,native_without_mana_cost_permission_forces_zero_x:true,
            }],
            ordered_costs:[NativeVariableManaPayment {
                native_printed_cost:cost,x_source:NativeXSource::NativePendingCastChoiceThenFinalizedStackStateXValue,
                each_printed_x_adds_chosen_x_generic_mana:true,total_cost_modifiers_require_current_native_payment_quote:true,
                payment_precedes_finalized_stack_publication:true,
            }],
            ordered_resolution:[PermanentEntry::ResolveExistingWildwoodCapturedXEntry],
            mana_value:NativeVariableManaValue {
                printed_base:d.mana_value,stack_x_multiplicity:d.cost.x_count,stack_uses_finalized_captured_x:true,
                pending_unfinalized_object_uses_printed_base:true,
                off_stack_x_value:0,maximum_represented_stack_value:d.mana_value+family.entry.represented_x_maximum*u16::from(d.cost.x_count),
            },
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            existing_entry_meaning_owns_the_only_counter_placement:true,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }],
    }})))
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
    // These producers publish closed records with at most an index, not a serialized
    // rules Root enum. Decode their actual transport and map only its emitted
    // selectors. The digest authenticates the canonical meaning after that
    // mapping; it never selects or supplies a missing root.
    macro_rules! indexed {
        ($transport:ty,$module:ident,$selector:ident,$function:ident,$row:ident,$argument:expr) => {{
            let $row: $transport = serde_json::from_value(Value::Object(record.clone()))
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            // In particular, Option::None must be the producer's explicit
            // null, not an erased selector silently defaulted by serde.
            if serde_json::to_value(&$row).map_err(|e| Error::Projection(e.to_string()))?
                != Value::Object(record.clone())
            {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            let selector =
                crate::$module::$selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            let meaning = crate::$module::$function(&selector, id, $argument)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            if $row.card_id != id || $row.root_sha256 != meaning.root_sha256 {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
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
        "Authority of the Consuls" => indexed!(
            crate::authority_public_v1::RootV1,
            rules_meaning_authority_v1,
            live_selector_v1,
            meaning_v1,
            row,
            match row.ability_index {
                None => crate::rules_meaning_authority_v1::RootV1::OpponentCreatureEntryTappedReplacement,
                Some(0) => crate::rules_meaning_authority_v1::RootV1::OpponentCreatureEntryGainLifeTrigger,
                _ => return Err(Error::UnsupportedPrintedRoot(id)),
            }
        ),
        "Involuntary Employment" => indexed!(
            crate::employment_public_v1::RootV1,
            rules_meaning_employment_v1,
            live_selector,
            meaning,
            row,
            crate::rules_meaning_employment_v1::RootV1::PrintedSpell
        ),
        "Slumbering Cerberus" => indexed!(
            crate::cerberus_public_v1::RootV1,
            rules_meaning_cerberus_v1,
            live_selector,
            meaning,
            row,
            match row.ability_index {
                0 => crate::rules_meaning_cerberus_v1::RootV1::ControllerUntapStepRestriction,
                1 => crate::rules_meaning_cerberus_v1::RootV1::PrintedEndStepTrigger { trigger_index: 0 },
                _ => return Err(Error::UnsupportedPrintedRoot(id)),
            }
        ),
        "Mild-Mannered Librarian" => typed!(
            rules_meaning_librarian_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Temporal Manipulation" => indexed!(
            crate::extra_turn_public_v1::RootV1,
            rules_meaning_extra_turn_v1,
            live_selector,
            meaning,
            row,
            match row.ability_index {
                0 => crate::rules_meaning_extra_turn_v1::RootV1::PrintedSpell,
                _ => return Err(Error::UnsupportedPrintedRoot(id)),
            }
        ),
        "Juggernaut" => indexed!(
            crate::juggernaut_public_v1::RootV1,
            rules_meaning_juggernaut_v1,
            live_selector,
            meaning,
            row,
            match row.ability_index {
                0 => crate::rules_meaning_juggernaut_v1::RootV1::MustAttackEachCombat,
                1 => crate::rules_meaning_juggernaut_v1::RootV1::CannotBeBlockedByWalls,
                _ => return Err(Error::UnsupportedPrintedRoot(id)),
            }
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
