//! A land play is not a spell; its separately paid fetch uses the stack.
use super::{Error, Result};
use crate::{
    card_def::{ActivationTargetFilter, CardDef, CardType, CostComponent, Keywords, TargetSpec},
    effect::{EffectOp, LibraryCardFilter, PlayerRef},
    mana::Cost,
    state::Zone,
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    name == "Evolving Wilds"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != Cost::zero()
        || d.mana_value != 0
        || d.types != [CardType::Land]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || !d.colors.is_empty()
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || !d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)().is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let [ability] = d.activated_abilities else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    if ability.cost != [CostComponent::Tap, CostComponent::SacrificeSelf]
        || ability.target_spec != TargetSpec::None
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || (ability.effect)()
            != (EffectOp::SearchLibraryToBattlefieldTapped {
                player: PlayerRef::Controller,
                filter: LibraryCardFilter::BasicLand,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(json!({"card_id":id,"evolving_wilds_printing":{
        "schema":"printed-land-play-sacrifice-search/v1","cost":Cost::zero(),"mana_value":0,"types":["Land"],"subtypes":[],"supertypes":[],"colors":[],"power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_land_play_roots":[{"origin":"OwnHand","actor":"PhysicalCardOwner","timing":"OwnMainPhaseEmptyStackWithPriority","is_cast":false,"uses_stack":false,"pays_mana":false,
            "requires_current_native_land_play_permission":true,"consumes_one_native_land_play":true,"normal_land_play_limits_and_additional_permissions_apply":true,
            "ordered_resolution":[{"operation":"MoveOriginalHandCardToBattlefield","controller":"LandPlayer","printed_enters_tapped":false,"normal_entry_replacements_and_triggers_apply":true}]}],
        "ordered_mana_roots":[],
        "ordered_activated_roots":[{"ability_index":0,"source_zone":"Battlefield","requires_active_printed_ability":true,"timing":"WithPriorityOnEitherPlayersTurn","uses_stack":true,"is_mana_ability":false,"targets":[],"controller":"CapturedAbilityControllerAtActivation",
            "ordered_costs":[{"operation":"TapOriginalAbilitySource","requires_untapped":true,"normal_creature_tap_cost_sickness_rules_apply_if_source_is_a_creature":true},{"operation":"SacrificeOriginalAbilitySource","requires_controlled_sacrificable_battlefield_incarnation":true,"normal_sacrifice_zone_replacements_apply":true}],
            "all_costs_paid_before_ability_publication":true,"no_mana_payment":true,"printed_activation_limit":null,"source_departure_or_later_graveyard_exile_does_not_remove_ability":true,"failed_or_countered_ability_does_not_refund_costs":true,
            "ordered_resolution":[{"operation":"SearchOwnLibrary","player":"CapturedAbilityControllerAtActivation","choice_visibility":"ChooserOnly","filter":"BasicLandCard","minimum":0,"maximum":1,"selection_is_not_targeting":true,"may_fail_to_find_even_when_matches_exist":true,"binds_original_live_selected_incarnation":true},
                {"operation":"MoveSelectedCardToBattlefieldTappedIfAny","controller":"CapturedAbilityControllerAtActivation","selected_card_becomes_public":true,"normal_entry_replacements_and_triggers_apply":true,"is_land_play":false,"does_not_consume_land_play_permission":true},
                {"operation":"ShuffleOwnLibrary","required_even_when_no_card_found":true}]}]
    }})))
}
