//! Exact controller-turn self keyword and controlled +1/+1-counter grant.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, ObjectRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    name == "Inspiring Paladin"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let cost = Cost {
        generic: 2,
        pips: &[Pip::Colored(ManaColor::W)],
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 3
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Human, Subtype::Knight]
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::W]
        || d.power != Some(3)
        || d.toughness != Some(3)
        || d.keywords != Keywords::NONE
        || d.controlled_counter_keyword != Some(Keywords::FIRST_STRIKE)
        || crate::engine::static_controller_turn_self_keyword_for(d.name)
            != Some(Keywords::FIRST_STRIKE)
        || !crate::engine::static_counter_keyword_controller_turn_only_for(d.name)
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
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
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","timing":"OwnMainPhaseEmptyStackWithPriority",
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"failed_or_countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let statics = json!([{"operation":"GrantKeywordToThisSource","keyword_bits":Keywords::FIRST_STRIKE.0,
        "source_zone":"Battlefield","source_face":0,"requires_active_printed_abilities":true,
        "condition":"ActivePlayerEqualsCurrentSourceController","does_not_require_counter":true,
        "normal_layers_and_timestamped_ability_removal_apply":true},
        {"operation":"GrantKeywordToCurrentControlledCreatureSet","keyword_bits":Keywords::FIRST_STRIKE.0,
        "source_zone":"Battlefield","source_face":0,"requires_active_printed_abilities":true,
        "condition":"ActivePlayerEqualsCurrentSourceController","controller":"CurrentSourceController",
        "recipient_type":"CurrentEffectiveCreature","required_counter":{"kind":"PlusOnePlusOne","count":"StrictlyPositive"},
        "other_counter_kinds_do_not_qualify":true,"opponents_creatures_excluded":true,
        "rechecks_current_control_type_and_counter_count":true,"includes_later_qualifying_creatures":true,
        "normal_layers_and_timestamped_ability_removal_apply":true,"ends_with_source_departure_or_inactive_printed_ability":true}]);
    Ok(Some(json!({"card_id":id,"paladin_printing":{
        "schema":"printed-controller-turn-counter-keyword/v1","cost":cost,"mana_value":3,"types":d.types,"subtypes":d.subtypes,
        "supertypes":[],"colors":d.colors,"power":3,"toughness":3,"keywords":0,"token":false,
        "ordered_cast_roots":[cast],"ordered_static_roots":statics}})))
}
