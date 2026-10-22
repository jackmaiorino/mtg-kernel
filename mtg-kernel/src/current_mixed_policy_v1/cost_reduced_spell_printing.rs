//! Closed printed discounts and their complete untargeted resolutions.
use super::{Error, Result};
use crate::{
    card_def::{
        CardDef, CardType, DynamicCountDef, GenericCostReductionDef, Keywords, Subtype, TargetSpec,
    },
    effect::{EffectOp, PlayerRef},
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Arcane Epiphany" | "Claws Out")
}
/// The caller exempts only this checked reduction from its shared exclusions.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let arcane = d.name == "Arcane Epiphany";
    let cost = Cost {
        generic: 3,
        pips: if arcane {
            &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)]
        } else {
            &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)]
        },
        x_count: 0,
    };
    let reduction = GenericCostReductionDef {
        generic_per_count: 1,
        count: if arcane {
            DynamicCountDef::ControllerHasPermanentSubtype(Subtype::Wizard)
        } else {
            DynamicCountDef::ControllerBattlefieldSubtype(Subtype::Cat)
        },
    };
    let program = if arcane {
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 3,
        }
    } else {
        EffectOp::BoostControlledCreaturesUntilEndOfTurn {
            power: 2,
            toughness: 2,
            keywords: Keywords::NONE,
        }
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 5
        || d.generic_cost_reduction != Some(reduction)
        || d.types != [CardType::Instant]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [if arcane { ManaColor::U } else { ManaColor::W }]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::None
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(program)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let count = if arcane {
        json!({"operation":"HasCurrentControlledPermanentWithSubtype","subtype":"Wizard",
        "count_is_zero_or_one":true,"multiple_wizards_do_not_increase_discount":true})
    } else {
        json!({"operation":"CountCurrentControlledPermanentsWithSubtype","subtype":"Cat",
            "each_permanent_counts_once":true,"tokens_count":true})
    };
    let resolution = if arcane {
        json!({"operation":"DrawCards","recipient":"CapturedSpellController","count":3,
        "normal_individual_draw_events_replacements_and_empty_library_rules_apply":true})
    } else {
        json!({"operation":"ModifyCurrentControlledCreatureSet","controller":"CapturedSpellController",
            "set_sampled_at_resolution":true,"power_delta":2,"toughness_delta":2,"keyword_bits":0,
            "places_no_counters":true,"binds_exact_current_battlefield_incarnations":true,
            "later_entering_creatures_not_included":true,"duration":"UntilCleanupExpirationAfterDiscard",
            "normal_continuous_layers_apply":true})
    };
    let cast_resolution = json!([resolution]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","timing":"InstantWithPriority",
            "ordered_costs":[{"operation":"PayNativeAdjustedPrintedMana","printed_cost":cost,
                "apply_checked_printed_generic_reduction":true}],"targets":[],
            "ordered_resolution":cast_resolution,"physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone","countering_skips_all_resolution_instructions":true,
            "countering_does_not_refund_cost":true,"other_cast_permissions_require_current_candidate_authority":true,
            "normal_zone_replacements_apply":true}]);
    Ok(Some(json!({"card_id":id,"cost_reduced_spell_printing":{
        "schema":"printed-cost-reduced-spell/v1","cost":cost,"mana_value":5,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
        "power":null,"toughness":null,"keywords":0,"token":false,
        "printed_generic_cost_reduction":{"generic_per_count":1,"count":count,
            "count_uses_current_native_effective_subtype_and_control":true,
            "opponents_permanents_and_nonbattlefield_cards_excluded":true,
            "sample_time":"Native601.2fCostLockBeforePayment",
            "normal_cost_increases_and_other_modifiers_apply":true,"floor_generic_at_zero":true,
            "colored_pips_unchanged":true,"printed_mana_value_unchanged":true,
            "later_subtype_control_or_zone_changes_do_not_reprice_finalized_cast":true},
        "ordered_cast_roots":cast_roots
    }})))
}
