//! Closed ordinary printed programs used by paid native numeric fixtures.
use super::{BaselineDeparture, BaselineManaCost, BaselineTiming, Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectCond, EffectOp, ObjectRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    state::{CastMethodV4, SpellCastRouteV4, Zone},
};

#[derive(serde::Serialize)]
enum TargetFilter {
    None,
    Creature,
    ArtifactPermanent,
}
#[derive(serde::Serialize)]
enum Resolution {
    DamageBoundCreature {
        target_slot: u8,
        amount: i32,
    },
    DestroyBoundArtifactIfStillOnBattlefield {
        target_slot: u8,
    },
    DamageEveryCreatureWithoutSubtype {
        amount: i32,
        excluded_subtype: u16,
        simultaneous_damage_batch: bool,
        evaluate_effective_types_at_resolution: bool,
    },
}
#[derive(serde::Serialize)]
pub(super) struct CastRoot {
    method: CastMethodV4,
    route: SpellCastRouteV4,
    origin: Zone,
    origin_owner_must_be_caster: bool,
    timing: BaselineTiming,
    mode_index: u8,
    ordered_costs: [BaselineManaCost; 1],
    target_count: u8,
    target_filter: TargetFilter,
    target_may_be_own_or_opposing: bool,
    target_requires_current_battlefield_incarnation: bool,
    all_targets_illegal_skips_entire_resolution: bool,
    ordered_resolution: [Resolution; 1],
    physical_stack_departure: BaselineDeparture,
    virtual_spell_copy_ceases: bool,
    other_cast_permissions_require_current_candidate_authority: bool,
}
fn root(cost: Cost, mode_index: u8, filter: TargetFilter, op: Resolution) -> CastRoot {
    let targeted = !matches!(filter, TargetFilter::None);
    CastRoot {
        method: CastMethodV4::Normal,
        route: SpellCastRouteV4::Hand,
        origin: Zone::Hand,
        origin_owner_must_be_caster: true,
        timing: BaselineTiming::InstantWithPriority,
        mode_index,
        ordered_costs: [BaselineManaCost::PrintedMana(cost)],
        target_count: u8::from(targeted),
        target_filter: filter,
        target_may_be_own_or_opposing: targeted,
        target_requires_current_battlefield_incarnation: targeted,
        all_targets_illegal_skips_entire_resolution: targeted,
        ordered_resolution: [op],
        physical_stack_departure: BaselineDeparture::GraveyardAfterResolutionOrCounter,
        virtual_spell_copy_ceases: true,
        other_cast_permissions_require_current_candidate_authority: true,
    }
}

/// Every other capability remains subject to the caller's common exclusion
/// guard. A familiar name never admits changed characteristics or programs.
pub(super) fn cast_roots(id: u16, d: &CardDef) -> Result<Option<Vec<CastRoot>>> {
    let cost = Cost {
        pips: &[Pip::Colored(ManaColor::R)],
        generic: match d.name {
            "Abrade" => 1,
            "Breath Weapon" => 2,
            _ => return Ok(None),
        },
        x_count: 0,
    };
    if d.is_token
        || d.is_land
        || d.types != [CardType::Instant]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::R]
        || d.cost != cost
        || d.mana_value != u16::from(cost.generic) + 1
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    match d.name {
        "Abrade" => {
            let destroy = EffectOp::Conditional {
                cond: EffectCond::TargetInZone(0, Zone::Battlefield),
                then: Box::new(EffectOp::DestroyObject {
                    object: ObjectRef::Target(0),
                }),
                else_: Box::new(EffectOp::Sequence(vec![])),
            };
            if d.target_spec != TargetSpec::Creature
                || (d.spell_effect)()
                    != Some(EffectOp::DealDamage {
                        target: TargetRef::Target(0),
                        amount: 3,
                    })
                || !d.mode2.as_ref().is_some_and(|mode| {
                    mode.target_spec == TargetSpec::ArtifactPermanent && (mode.effect)() == destroy
                })
            {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            Ok(Some(vec![
                root(
                    cost,
                    0,
                    TargetFilter::Creature,
                    Resolution::DamageBoundCreature {
                        target_slot: 0,
                        amount: 3,
                    },
                ),
                root(
                    cost,
                    1,
                    TargetFilter::ArtifactPermanent,
                    Resolution::DestroyBoundArtifactIfStillOnBattlefield { target_slot: 0 },
                ),
            ]))
        }
        "Breath Weapon" => {
            if d.target_spec != TargetSpec::None
                || d.mode2.is_some()
                || (d.spell_effect)()
                    != Some(EffectOp::DamageEachCreatureWithoutSubtype {
                        amount: 2,
                        excluded_subtype: Subtype::Dragon,
                    })
            {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            Ok(Some(vec![root(
                cost,
                0,
                TargetFilter::None,
                Resolution::DamageEveryCreatureWithoutSubtype {
                    amount: 2,
                    excluded_subtype: Subtype::Dragon as u16,
                    simultaneous_damage_batch: true,
                    evaluate_effective_types_at_resolution: true,
                },
            )]))
        }
        _ => unreachable!(),
    }
}
