//! Closed Shivan printing, including the actual paid incarnation-bound pump.
use super::{Error, Result};
use crate::{
    card_def::{
        ActivationTargetFilter, CardDef, CardType, CostComponent, DynamicValueDef, Keywords,
        Subtype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    name == "Shivan Dragon"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let cost = Cost {
        generic: 4,
        pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    let red = Cost {
        generic: 0,
        pips: &[Pip::Colored(ManaColor::R)],
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 6
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Dragon]
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::R]
        || d.power != Some(5)
        || d.toughness != Some(5)
        || d.keywords != Keywords::FLYING
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
    if ability.cost != [CostComponent::Mana(red)]
        || ability.target_spec != TargetSpec::None
        || (ability.effect)()
            != (EffectOp::PumpTargetUntilEndOfTurnDynamic {
                target: TargetRef::ThisSource,
                power: DynamicValueDef::Fixed(1),
                toughness: DynamicValueDef::Fixed(0),
            })
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","timing":"OwnMainPhaseEmptyStackWithPriority",
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"failed_or_countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let activated = json!([{"ability_index":0,"source_zone":"Battlefield","requires_active_printed_ability":true,
        "timing":"WithPriorityOnEitherPlayersTurn","sorcery_only":false,"uses_stack":true,"targets":[],
        "ordered_costs":[{"operation":"PayMana","cost":red}],"tap_cost":false,"summoning_sickness_does_not_prohibit_activation":true,
        "printed_activation_limit":null,"each_activation_is_independently_paid_before_publication":true,
        "ordered_resolution":[{"operation":"ModifyOriginalAbilitySourcePowerToughness","binding":"CapturedBattlefieldIncarnation",
            "requires_original_source_still_on_battlefield_with_same_incarnation":true,"power_delta":1,"toughness_delta":0,
            "layer":"7c","duration":"UntilCleanupExpirationAfterDiscard","places_no_counters":true,
            "surviving_activations_add_independently":true}],
        "source_departure_does_not_remove_ability_from_stack":true,"departed_or_reentered_source_receives_no_boost":true,
        "failed_or_countered_ability_does_not_refund_cost":true}]);
    Ok(Some(json!({"card_id":id,"shivan_dragon_printing":{
        "schema":"printed-red-self-pump-dragon/v1","cost":cost,"mana_value":6,"types":d.types,"subtypes":d.subtypes,
        "supertypes":[],"colors":d.colors,"power":5,"toughness":5,"keywords":d.keywords.0,"token":false,
        "ordered_cast_roots":[cast],"ordered_activated_roots":activated}})))
}
