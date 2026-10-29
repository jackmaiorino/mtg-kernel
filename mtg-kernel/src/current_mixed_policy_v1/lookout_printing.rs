//! Tap is an activation cost; the private discard follows the resolved draw.
use super::{Error, Result};
use crate::{
    card_def::{
        ActivationTargetFilter, CardDef, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    name == "Strix Lookout"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let cost = Cost {
        generic: 1,
        pips: &[Pip::Colored(ManaColor::U)],
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 2
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Bird]
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::U]
        || d.power != Some(1)
        || d.toughness != Some(2)
        || d.keywords != Keywords(Keywords::FLYING.0 | Keywords::VIGILANCE.0)
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !crate::trigger::triggers_for(id).is_empty()
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
    let [ability] = d.activated_abilities else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    if ability.cost != [CostComponent::Mana(cost), CostComponent::Tap]
        || ability.target_spec != TargetSpec::None
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || (ability.effect)()
            != EffectOp::Sequence(vec![
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                EffectOp::DiscardCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
            ])
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(json!({"card_id":id,"strix_lookout_printing":{
        "schema":"printed-tap-draw-then-discard/v1","cost":cost,"mana_value":2,"types":d.types,"subtypes":d.subtypes,
        "supertypes":[],"colors":d.colors,"power":1,"toughness":2,"keywords":d.keywords.0,"token":false,
        "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
            "timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
            "countering_skips_entry_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
            "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true}],
        "ordered_activated_roots":[{"ability_index":0,"source_zone":"Battlefield","requires_active_printed_ability":true,
            "timing":"WithPriorityOnEitherPlayersTurn","uses_stack":true,"targets":[],"controller":"CapturedAbilityControllerAtActivation",
            "ordered_costs":[{"operation":"PayMana","cost":cost},{"operation":"TapOriginalAbilitySource",
                "requires_untapped":true,"normal_tap_cost_summoning_sickness_and_haste_rules_apply":true}],
            "printed_activation_limit":null,"each_activation_is_independently_paid_before_publication":true,
            "source_departure_does_not_remove_ability_from_stack":true,"discard_is_not_activation_cost":true,
            "ordered_resolution":[{"operation":"DrawCards","recipient":"CapturedAbilityControllerAtActivation","count":1,
                "normal_draw_replacements_and_empty_library_rules_apply":true,"drawn_identity_is_visible_only_to_authorized_viewers":true},
                {"operation":"DiscardCards","player":"CapturedAbilityControllerAtActivation","requested_count":1,
                "clamp_to_available_hand":true,"private_choice_by_controller":true,"may_choose_just_drawn_card":true,
                "normal_discard_replacements_apply":true}],
            "failed_or_countered_ability_does_not_refund_cost":true}]
    }})))
}
