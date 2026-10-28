//! Closed mandatory discard-to-cast draw spell.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, CostComponent, Keywords, TargetSpec},
    effect::{EffectOp, PlayerRef},
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    name == "Thrill of Possibility"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let cost = Cost {
        generic: 1,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 2
        || d.types != [CardType::Instant]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::R]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || d.additional_cost != Some(&[CostComponent::DiscardCards(1)][..])
        || (d.spell_effect)()
            != Some(EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 2,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let costs = json!([{"operation":"DiscardOwnHandCards","count":1,"mandatory":true,"chooser":"CastingPlayer",
                        "any_card_type_including_land":true,"announced_spell_is_no_longer_hand_card":true,
                        "selection_is_private_to_payer_until_discard_occurs":true,
                        "normal_discard_replacements_and_triggers_apply":true},
                    {"operation":"PayNativeAdjustedPrintedMana","printed_cost":cost}]);
    let resolution = json!([{"operation":"DrawCards","recipient":"CapturedSpellController","count":2,
                    "individual_draw_events_in_order":true,"normal_draw_replacements_and_empty_library_loss_apply":true,
                    "card_identities_stay_with_authorized_viewers":true}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
                "timing":"InstantWithPriority","targets":[],
                "ordered_costs":costs,
                "native_cast_staging_selects_discard_before_final_mana_payment":true,
                "no_priority_between_cost_selection_and_complete_payment":true,
                "requires_entire_additional_cost_payable":true,"all_costs_paid_before_response_priority":true,
                "ordered_resolution":resolution,
                "countering_skips_draws_but_does_not_refund_mana_or_discard":true,
                "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
                "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone","copies_created_on_stack_do_not_pay_cast_costs":true,
                "other_cast_permissions_require_current_candidate_authority":true,"normal_zone_replacements_apply":true}]);
    Ok(Some(
        json!({"card_id":id,"mandatory_discard_draw_printing":{
            "schema":"printed-mandatory-discard-draw/v1","cost":cost,"mana_value":2,
            "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,
            "power":null,"toughness":null,"keywords":0,"token":false,
            "ordered_cast_roots":cast_roots
        }}),
    ))
}
