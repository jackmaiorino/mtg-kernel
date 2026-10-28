//! Closed Luminous Rebuke target discount and complete destruction meaning.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, DynamicCountDef, GenericCostReductionDef, Keywords, TargetSpec},
    effect::{EffectCond, EffectOp, ObjectRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    name == "Luminous Rebuke"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let cost = Cost {
        generic: 4,
        pips: &[Pip::Colored(ManaColor::W)],
        x_count: 0,
    };
    let program = EffectOp::Conditional {
        cond: EffectCond::TargetInZone(0, Zone::Battlefield),
        then: Box::new(EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 5
        || d.generic_cost_reduction
            != Some(GenericCostReductionDef {
                generic_per_count: 3,
                count: DynamicCountDef::SpellTargetsTappedCreature,
            })
        || d.types != [CardType::Instant]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::W]
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
    let cast_resolution = json!([{"operation":"DestroyPermanent","target_slot":0,
                "normal_indestructible_and_zone_replacements_apply":true}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","timing":"InstantWithPriority",
            "ordered_costs":[{"operation":"PayNativeAdjustedPrintedMana","printed_cost":cost,"apply_checked_target_discount":true}],
            "target_count":1,"target_slot":0,"target_zone":"Battlefield","target_type":"CurrentEffectiveCreature",
            "target_controller_relation":"EitherPlayerIncludingCaster","untapped_creatures_remain_legal_targets":true,
            "original_target_incarnation_and_creature_type_rechecked_at_resolution":true,
            "tapped_status_is_not_a_resolution_legality_requirement":true,
            "all_targets_illegal_skips_entire_resolution":true,
            "ordered_resolution":cast_resolution,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard","virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
            "countering_does_not_refund_cost":true,"other_cast_permissions_require_current_candidate_authority":true}]);
    Ok(Some(json!({"card_id":id,"luminous_rebuke_printing":{
        "schema":"printed-target-tap-discount-spell/v1","cost":cost,"mana_value":5,"types":d.types,
        "subtypes":[],"supertypes":[],"colors":d.colors,"power":null,"toughness":null,"keywords":0,"token":false,
        "printed_generic_cost_reduction":{"operation":"ReduceGenericByThreeIfChosenTargetIsTappedCreature",
            "target_slot":0,"zone":"Battlefield","creature_type":"CurrentEffectiveTypeIncludingAnimatedLands",
            "sample_time":"Native601.2fCostLockBeforeManaAbilitiesAndPayment","count_is_zero_or_one":true,
            "normal_cost_increases_and_other_modifiers_apply":true,"floor_generic_at_zero":true,
            "colored_pip_and_printed_mana_value_unchanged":true,
            "later_tap_or_untap_during_payment_or_after_finalization_does_not_reprice_cast":true},
        "ordered_cast_roots":cast_roots
    }})))
}
