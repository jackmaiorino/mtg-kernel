//! Definition-owned Fishing Pole grants, named costs and real untap lifecycle.
use crate::card_def::{self, CostComponent, Keywords, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Zone};

pub(crate) fn selected_definition(id: u16) -> bool {
    CARD_DEFS.get(usize::from(id)).is_some_and(|def| {
        def.is_executable()
            && def.name == "Fishing Pole"
            && def.equipment.is_some_and(|equipment| {
                let expected = profile().granted_activated_ability.unwrap();
                equipment.granted_activated_ability.is_some_and(|grant| {
                    grant.cost == expected.cost
                        && grant.target_spec == expected.target_spec
                        && (grant.effect)() == (expected.effect)()
                })
            })
    })
}

pub(crate) fn grants(state: &GameState, host: ObjectId) -> Vec<ObjectId> {
    let Some(creature) = state.objects.try_get(host) else {
        return Vec::new();
    };
    if creature.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, host, card_def::CardType::Creature)
    {
        return Vec::new();
    }
    crate::engine::attached_equipment_profiles(state, host)
        .filter(|(id, profile)| {
            let object = state.objects.get(*id);
            selected_definition(object.card_def)
                && profile.granted_activated_ability.is_some_and(|ability| {
                    (ability.effect)() == crate::effect::EffectOp::FishingPoleAddBaitV39
                })
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
                && crate::continuous_characteristics_v1::grant_survives(
                    state,
                    host,
                    object.v4.layer_timestamp.unwrap_or(0),
                )
        })
        .map(|(id, _)| id)
        .collect()
}

/// Each named Pole choice identifies one otherwise equivalent grant. It is
/// not a target and never includes another Equipment or an opposing Pole.
pub(crate) fn cost_candidates(state: &GameState, actor: PlayerId, host: ObjectId) -> Vec<ObjectId> {
    grants(state, host)
        .into_iter()
        .filter(|id| {
            let pole = state.objects.get(*id);
            pole.controller == actor
                && !pole.tapped
                && *id != host
                && pole.counters.bait.checked_add(1).is_some()
        })
        .collect()
}

pub(crate) fn granted_ability(
    state: &GameState,
    host: ObjectId,
) -> Option<card_def::ActivatedAbilityDef> {
    let first = grants(state, host).into_iter().next()?;
    let grant = CARD_DEFS[state.objects.get(first).card_def as usize]
        .equipment?
        .granted_activated_ability?;
    Some(card_def::ActivatedAbilityDef {
        cost: grant.cost,
        target_spec: grant.target_spec,
        effect: grant.effect,
        activation_zone: Zone::Battlefield,
        sorcery_speed_only: false,
        activation_target_filter: card_def::ActivationTargetFilter::TargetSpecOnly,
        max_activations_per_turn: None,
        face: None,
    })
}

pub(crate) fn named_cost(components: &[CostComponent]) -> bool {
    components.contains(&CostComponent::TapFishingPoleAttachedToSource)
}

pub(crate) const fn printed_grant_cost() -> &'static [CostComponent] {
    const COST: &[CostComponent] = &[
        CostComponent::Mana(crate::mana::Cost {
            generic: 1,
            pips: &[],
            x_count: 0,
        }),
        CostComponent::Tap,
        CostComponent::TapFishingPoleAttachedToSource,
    ];
    COST
}

pub(crate) const fn profile() -> card_def::EquipmentDef {
    card_def::EquipmentDef {
        power_delta: 0,
        toughness_delta: 0,
        pt_controller_turn_only: false,
        add_subtype: None,
        controller_turn_keywords: Keywords::NONE,
        other_turn_keywords: Keywords::NONE,
        noncreature_spell_damage_to_each_opponent: 0,
        job_select: false,
        granted_activated_ability: Some(card_def::GrantedActivatedAbilityDef {
            cost: printed_grant_cost(),
            target_spec: card_def::TargetSpec::None,
            effect: card_def::fishing_pole_bait,
        }),
    }
}

pub(crate) mod lifecycle;

#[cfg(test)]
mod tests;
