//! Unregistered pinned Eaten Alive recipe. No registry identity is assigned.
use crate::card_def::{CardType, CostComponent, Keywords, TargetSpec};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
pub const PRINTED: Cost = Cost {
    generic: 0,
    pips: &[Pip::Colored(ManaColor::B)],
    x_count: 0,
};
pub const MANA_BRANCH: &[CostComponent] = &[CostComponent::Mana(Cost {
    generic: 3,
    pips: &[Pip::Colored(ManaColor::B)],
    x_count: 0,
})];
pub const ADDITIONAL: &[CostComponent] = &[CostComponent::SacrificeCreatureOrPayMana(MANA_BRANCH)];
pub fn program() -> EffectOp {
    EffectOp::MoveObject {
        object: ObjectRef::Target(0),
        to_zone: crate::state::Zone::Exile,
    }
}
pub(crate) fn exact_recipe(id: u16) -> Result<(), String> {
    let d = crate::card_def::CARD_DEFS
        .get(usize::from(id))
        .ok_or("Eaten Alive definition absent")?;
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || !d.has_full_support()
        || d.name != "Eaten Alive"
        || d.cost != PRINTED
        || d.mana_value != 1
        || d.types != [CardType::Sorcery]
        || d.colors != [ManaColor::B]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.keywords != Keywords::NONE
        || d.power.is_some()
        || d.toughness.is_some()
        || d.is_token
        || d.is_land
        || d.target_spec != TargetSpec::CreatureOrPlaneswalker
        || (d.spell_effect)() != Some(program())
        || d.additional_cost != Some(ADDITIONAL)
        || d.alt_cost.is_some()
        || d.optional_additional_cost.is_some()
        || d.kicker_cost.is_some()
        || d.flashback.is_some()
        || d.escape.is_some()
        || d.mode2.is_some()
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.starting_loyalty.is_some()
        || d.controlled_creature_grant.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.plot_cost.is_some()
        || d.madness_cost.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.kicked_target_spec.is_some()
        || d.omen.is_some()
        || d.adventure.is_some()
        || d.bestow.is_some()
        || d.delve
        || d.attachment.is_some()
        || d.transform_face.is_some()
        || d.saga.is_some()
        || d.changeling
        || d.animation.is_some()
        || d.damage_replacement_v1.is_some()
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
    {
        return Err("Eaten Alive differs from pinned B/additional-sacrifice-or-3B recipe".into());
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
