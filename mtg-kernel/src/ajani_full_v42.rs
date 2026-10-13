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
