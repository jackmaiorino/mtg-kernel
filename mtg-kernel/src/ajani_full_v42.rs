//! Full Ajani source preparation, distinct from the frozen Partial recipient.
use crate::card_def::{
    CardCapability, CardType, CostComponent, DynamicValueDef, Keywords, Subtype, Supertype,
    TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;

pub const LIFE_COUNT: DynamicValueDef = DynamicValueDef::ControllerLifeTotal;
pub fn counter_program() -> EffectOp {
    EffectOp::AjaniLoyaltyV42 { ability_index: 0 }
}
pub fn strike_program() -> EffectOp {
    EffectOp::AjaniLoyaltyV42 { ability_index: 1 }
}
pub fn cats_program() -> EffectOp {
    EffectOp::AjaniLoyaltyV42 { ability_index: 2 }
}

pub(crate) fn selected_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.name == "Ajani, Caller of the Pride" && d.capability == CardCapability::Full
    })
}

pub(crate) fn validate_definition(id: u16) -> Result<(), String> {
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or("Ajani definition absent")?;
    if !selected_definition(id)
        || d.cost.generic != 1
        || d.cost.x_count != 0
        || d.mana_value != 3
        || d.changeling
        || d.transform_face.is_some()
        || d.animation.is_some()
        || d.cost.pips != [Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)]
        || d.colors != [ManaColor::W]
        || d.types != [CardType::Planeswalker]
        || d.subtypes != [Subtype::Ajani]
        || d.supertypes != [Supertype::Legendary]
        || d.starting_loyalty != Some(4)
        || d.power.is_some()
        || d.toughness.is_some()
        || d.is_land
        || d.is_token
        || d.keywords != Keywords::NONE
        || !d.is_executable()
        || d.object_name != d.name
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || d.mana_ability_def.is_some()
        || !d.additional_mana_abilities.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.alt_cost.is_some()
        || d.additional_cost.is_some()
        || d.optional_additional_cost.is_some()
        || d.kicker_cost.is_some()
        || d.kicked_target_spec.is_some()
        || d.flashback.is_some()
        || d.escape.is_some()
        || d.madness_cost.is_some()
        || d.plot_cost.is_some()
        || d.bestow.is_some()
        || d.omen.is_some()
        || d.adventure.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.transform_face.is_some()
        || d.attachment.is_some()
        || d.equipment.is_some()
        || d.animation.is_some()
        || d.saga.is_some()
        || d.changeling
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant.is_some()
        || d.enters_battlefield_tapped
        || d.enters_battlefield_tapped_unless.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || d.ward_cost.is_some()
        || d.generic_cost_reduction.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.delve
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || d.conditional_tap_yield.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.minimum_blockers > 1
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || d.activated_abilities.len() != 3
        || !crate::trigger::triggers_for(id).is_empty()
    {
        return Err("full Ajani printed definition differs".into());
    }
    for (index, delta, target) in [
        (
            0usize,
            1,
            TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneCreature),
        ),
        (1, -3, TargetSpec::Creature),
        (2, -8, TargetSpec::None),
    ] {
        let a = &d.activated_abilities[index];
        if a.cost != [CostComponent::Loyalty(delta)]
            || a.target_spec != target
            || a.activation_zone != Zone::Battlefield
            || !a.sorcery_speed_only
            || a.face.is_some()
            || a.max_activations_per_turn.is_some()
            || a.activation_target_filter != crate::card_def::ActivationTargetFilter::TargetSpecOnly
            || (a.effect)()
                != (EffectOp::AjaniLoyaltyV42 {
                    ability_index: index as u8,
                })
        {
            return Err("full Ajani loyalty recipe differs".into());
        }
    }
    Ok(())
}

pub(crate) fn token_definition() -> Result<u16, String> {
    let id = crate::card_def::card_id_by_name("Ajani Cat Token")
        .ok_or("Ajani requires its actual 2/2 white Cat blueprint")?;
    let d = &CARD_DEFS[usize::from(id)];
    if !d.is_token
        || !d.has_full_support()
        || !d.is_executable()
        || d.object_name != "Cat Token"
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Cat]
        || d.colors != [ManaColor::W]
        || d.power != Some(2)
        || d.toughness != Some(2)
        || !d.supertypes.is_empty()
        || d.keywords != Keywords::NONE
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || d.cost.generic != 0
        || d.cost.x_count != 0
        || !d.cost.pips.is_empty()
    {
        return Err("Ajani token differs from printed 2/2 white Cat".into());
    }
    Ok(id)
}

pub mod lifecycle;

#[cfg(test)]
mod tests;
