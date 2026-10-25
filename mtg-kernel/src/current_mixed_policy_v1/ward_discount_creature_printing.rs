//! Closed ward and generic-discount creatures, including Prowler's morbid root.
use super::{Error, Result};
use crate::{
    card_def::{
        CardDef, CardType, DynamicCountDef, GenericCostReductionDef, Keywords, Subtype, Supertype,
        TargetSpec, WardCostDef,
    },
    effect::{EffectCond, EffectOp, ObjectRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::TriggerCondition,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Cackling Prowler" | "Tolarian Terror" | "Sire of Seven Deaths" | "Ghalta, Primal Hunger"
    )
}

/// All unrelated CardDef fields remain subject to the caller's exclusions.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (generic, pips, colors, subtypes, supertypes, power, toughness, keywords, ward, count) =
        match d.name {
            "Cackling Prowler" => (
                3,
                &[Pip::Colored(ManaColor::G)][..],
                &[ManaColor::G][..],
                &[Subtype::Hyena, Subtype::Rogue][..],
                &[][..],
                4,
                3,
                Keywords::NONE,
                Some(WardCostDef::Generic(2)),
                None,
            ),
            "Tolarian Terror" => (
                6,
                &[Pip::Colored(ManaColor::U)][..],
                &[ManaColor::U][..],
                &[Subtype::Serpent][..],
                &[][..],
                5,
                5,
                Keywords::NONE,
                Some(WardCostDef::Generic(2)),
                Some(DynamicCountDef::ControllerGraveyardAnyType(&[
                    CardType::Instant,
                    CardType::Sorcery,
                ])),
            ),
            "Sire of Seven Deaths" => (
                7,
                &[][..],
                &[][..],
                &[Subtype::Eldrazi][..],
                &[][..],
                7,
                7,
                Keywords::FIRST_STRIKE
                    | Keywords::VIGILANCE
                    | Keywords::MENACE
                    | Keywords::TRAMPLE
                    | Keywords::REACH
                    | Keywords::LIFELINK,
                Some(WardCostDef::PayLife(7)),
                None,
            ),
            "Ghalta, Primal Hunger" => (
                10,
                &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)][..],
                &[ManaColor::G][..],
                &[Subtype::Elder, Subtype::Dinosaur][..],
                &[Supertype::Legendary][..],
                12,
                12,
                Keywords::TRAMPLE,
                None,
                Some(DynamicCountDef::ControllerCreatureTotalPower),
            ),
            _ => return Ok(None),
        };
    let cost = Cost {
        generic,
        pips,
        x_count: 0,
    };
    let reduction = count.map(|count| GenericCostReductionDef {
        generic_per_count: 1,
        count,
    });
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(generic) + pips.len()
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || d.supertypes != supertypes
        || d.colors != colors
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != keywords
        || d.ward_cost != ward
        || d.generic_cost_reduction != reduction
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
    let mut triggers = vec![];
    if d.name == "Cackling Prowler" {
        let program = EffectOp::Conditional {
            cond: EffectCond::CreatureDiedThisTurn,
            then: Box::new(EffectOp::BindPlusOnePlusOneCounterToTriggerSource),
            else_: Box::new(EffectOp::Sequence(vec![])),
        };
        let [t] = crate::trigger::triggers_for(id) else {
            return Err(Error::UnsupportedPrintedRoot(id));
        };
        if t.condition != TriggerCondition::BeginningControllerEndStepIfCreatureDied
            || t.home_zone != Zone::Battlefield
            || t.face_index != 0
            || t.intervening_if_kicked
            || t.intervening_if_controls_another_source_card
            || (t.effect)() != program
            || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        triggers.push(json!({"event":"BeginningOfCurrentControllersEndStep","controller":"ControllerAtTriggerTime",
            "home_zone":"Battlefield","source_face":0,"mandatory":true,"targets":[],"uses_stack":true,
            "intervening_if":{"operation":"AnyCreatureDiedThisTurn","checks_at_trigger_and_resolution":true,
                "either_controllers_creatures_and_tokens_count":true,"death_means_battlefield_to_graveyard":true},
            "ordered_resolution":[{"operation":"PutPlusOnePlusOneCounterOnOriginalTriggerSource","count":1,
                "requires_same_battlefield_incarnation":true,"missing_or_reentered_source_gets_no_counter":true,
                "normal_counter_replacements_and_triggers_apply":true}]}));
    } else if !crate::trigger::triggers_for(id).is_empty() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let ward_meaning = match ward {
        None => Value::Null,
        Some(payment) => json!({"event":"ThisPermanentBecomesTargetOfOpponentsSpellOrAbility",
            "controller":"ControllerAtTargetingEvent","targets":[],"uses_stack":true,
            "binding":"OriginalTargetingStackItemAndWardSourceIncarnation",
            "survives_source_departure_and_later_control_change":true,
            "ordered_resolution":[{"operation":"CounterOriginalTargetingStackItemUnlessItsControllerPays",
                "payment":match payment { WardCostDef::Generic(n) => json!({"kind":"GenericMana","amount":n}),
                    WardCostDef::PayLife(n) => json!({"kind":"Life","amount":n}), _ => unreachable!() },
                "payment_is_optional":true,"payment_uses_current_native_legality_and_mana_abilities":true,
                "normal_cannot_be_countered_rules_apply":true,"no_refund_of_original_spell_or_ability_costs":true}]}),
    };
    let count_meaning = match count {
        None => Value::Null,
        Some(DynamicCountDef::ControllerGraveyardAnyType(_)) => {
            json!({"operation":"CountCardsInCastersGraveyardWithAnyPrintedType",
            "types":["Instant","Sorcery"],"each_card_counts_once":true,"opponents_graveyard_excluded":true})
        }
        Some(DynamicCountDef::ControllerCreatureTotalPower) => {
            json!({"operation":"SumEffectivePowerOfCurrentControlledBattlefieldCreatures",
            "tokens_and_animated_noncreature_cards_count":true,"negative_powers_contribute_before_floor":true,
            "sum_floor":0,"sum_ceiling":u32::MAX,"opponents_creatures_excluded":true})
        }
        _ => unreachable!(),
    };
    let discount = if count.is_some() {
        json!({"generic_per_count":1,"count":count_meaning,
        "sample_time":"Native601.2fCostLockBeforeManaAbilitiesAndPayment","floor_generic_at_zero":true,
        "colored_pips_and_printed_mana_value_unchanged":true,"normal_cost_increases_and_modifiers_apply":true,
        "later_state_changes_do_not_reprice_finalized_cast":true})
    } else {
        Value::Null
    };
    Ok(Some(
        json!({"card_id":id,"ward_discount_creature_printing":{
            "schema":"printed-ward-discount-creature/v1","cost":cost,"mana_value":d.mana_value,
            "types":d.types,"subtypes":d.subtypes,"supertypes":d.supertypes.iter().map(|s| *s as u8).collect::<Vec<_>>(),"colors":d.colors,
            "power":power,"toughness":toughness,"keywords":keywords.0,"token":false,
            "printed_generic_cost_reduction":discount,"ward_trigger":ward_meaning,"ordered_triggers":triggers,
            "ordinary_cast":{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
                "timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],
                "ordered_costs":[{"operation":"PayNativeAdjustedPrintedMana","printed_cost":cost,
                    "apply_checked_printed_generic_reduction":count.is_some()}],
                "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
                "countered_or_failed_spell_destination":"OwnersGraveyard","countering_does_not_refund_cost":true,
                "other_cast_permissions_require_current_candidate_authority":true,"normal_zone_replacements_and_entry_triggers_apply":true}
        }}),
    ))
}
