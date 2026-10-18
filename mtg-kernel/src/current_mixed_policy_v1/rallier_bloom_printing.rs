//! Exact ordinary Squad Rallier and Bloom Tender supplements, without executable operands.
use super::*;
use crate::card_def::{
    ActivationTargetFilter, CostComponent, Keywords, ManaAbilityAmountDef, ManaAbilityCostDef,
    ManaAbilityDef, Subtype,
};
use crate::mana::{Cost, ManaColor, Pip};
fn validate_shape(id: u16, d: &CardDef) -> Result<bool> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let bloom = match d.name {
        "Bloom Tender" => true,
        "Squad Rallier" => false,
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    if CARD_DEFS
        .get(usize::from(id))
        .is_none_or(|actual| actual.name != d.name)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cost = Cost {
        generic: if bloom { 1 } else { 3 },
        pips: if bloom {
            &[Pip::Colored(ManaColor::G)]
        } else {
            &[Pip::Colored(ManaColor::W)]
        },
        x_count: 0,
    };
    let mana_value = if bloom { 2 } else { 4 };
    let subtypes = if bloom {
        &[Subtype::Elf, Subtype::Druid][..]
    } else {
        &[Subtype::Human, Subtype::Scout][..]
    };
    let colors = if bloom {
        &[ManaColor::G][..]
    } else {
        &[ManaColor::W][..]
    };
    let crate::card_def::CardDef {
        name: _,
        capability: _,
        cost: _,
        generic_cost_reduction: _,
        ward_cost: _,
        spell_cannot_be_countered: _,
        records_mana_spent: _,
        equipment: _,
        types: _,
        subtypes: _,
        supertypes: _,
        power: _,
        toughness: _,
        starting_loyalty: _,
        enters_with_plus_one_counters: _,
        controlled_counter_keyword: _,
        game_outcome_protection_v1: _,
        controlled_creature_grant: _,
        is_land: _,
        produces_mana: _,
        colors: _,
        target_spec: _,
        keywords: _,
        spell_effect: _,
        mana_ability: _,
        alt_cost: _,
        kicker_cost: _,
        additional_cost: _,
        flashback: _,
        activated_abilities: _,
        plot_cost: _,
        madness_cost: _,
        mode2: _,
        mode3: _,
        additional_modes: _,
        kicked_target_spec: _,
        is_token: _,
        escape: _,
        mana_ability_choices: _,
        enters_battlefield_tapped: _,
        mana_ability_def: _,
        minimum_blockers: _,
        omen: _,
        mana_value: _,
        mana_ability_includes_chosen_color: _,
        as_enters_choose_color_other_than: _,
        additional_mana_abilities: _,
        object_name: _,
        enters_battlefield_tapped_unless: _,
        attachment: _,
        transform_face: _,
        saga: _,
        optional_additional_cost: _,
        changeling: _,
        bestow: _,
        delve: _,
        adventure: _,
        cant_be_blocked_by_monarchs_creatures: _,
        conditional_tap_yield: _,
        enters_tapped_unless_controller: _,
        additional_mana_ability_conditions: _,
        restricted_mana_abilities: _,
        animation: _,
        activated_ability_generic_reductions: _,
        combat_static_rules: _,
        as_enters_color_choice_v1: _,
        chosen_color_creature_boost_v1: _,
        damage_replacement_v1: _,
        token_counter_replacement_v48: _,
        starting_life_self_bonuses_v1: _,
        source_type_hexproof_v1: _,
        other_controlled_nontoken_death_v1: _,
    } = d;
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != cost
        || d.mana_value != mana_value
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || d.colors != colors
        || !d.supertypes.is_empty()
        || d.power != Some(if bloom { 1 } else { 3 })
        || d.toughness != Some(if bloom { 1 } else { 4 })
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || d.produces_mana
            != if bloom {
                &[
                    ManaColor::W,
                    ManaColor::U,
                    ManaColor::B,
                    ManaColor::R,
                    ManaColor::G,
                ][..]
            } else {
                &[][..]
            }
        || (d.mana_ability)()
            != if bloom {
                Some(EffectOp::Sequence(vec![
                    EffectOp::TapObject {
                        object: ObjectRef::ThisSource,
                    },
                    EffectOp::AddMana {
                        player: PlayerRef::Controller,
                        colors: vec![ManaColor::G],
                    },
                ]))
            } else {
                None
            }
        || d.mana_ability_choices != if bloom { &[ManaColor::G][..] } else { &[][..] }
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.attachment.is_some()
        || d.starting_loyalty.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.alt_cost.is_some()
        || d.kicker_cost.is_some()
        || d.additional_cost.is_some()
        || d.flashback.is_some()
        || d.plot_cost.is_some()
        || d.madness_cost.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.kicked_target_spec.is_some()
        || d.escape.is_some()
        || d.enters_battlefield_tapped
        || d.mana_ability_def
            != if bloom {
                Some(ManaAbilityDef {
                    cost: ManaAbilityCostDef::TapSelf,
                    amount: ManaAbilityAmountDef::EachControlledPermanentColor,
                    controller_damage: 0,
                    max_activations_per_turn: None,
                })
            } else {
                None
            }
        || d.omen.is_some()
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || !d.additional_mana_abilities.is_empty()
        || d.enters_battlefield_tapped_unless.is_some()
        || d.transform_face.is_some()
        || d.saga.is_some()
        || d.optional_additional_cost.is_some()
        || d.changeling
        || d.bestow.is_some()
        || d.delve
        || d.adventure.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.conditional_tap_yield.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || d.animation.is_some()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.as_enters_color_choice_v1.is_some()
        || d.chosen_color_creature_boost_v1.is_some()
        || d.damage_replacement_v1.is_some()
        || d.token_counter_replacement_v48.is_some()
        || !d.starting_life_self_bonuses_v1.is_empty()
        || d.source_type_hexproof_v1.is_some()
        || d.other_controlled_nontoken_death_v1.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    if !crate::trigger::triggers_for(id).is_empty() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    if bloom {
        if !d.activated_abilities.is_empty() {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    } else {
        if d.activated_abilities.len() != 1 {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        let a = &d.activated_abilities[0];
        if !matches!(a.cost, [CostComponent::Mana(cost)] if *cost == Cost { generic:2,pips:&[Pip::Colored(ManaColor::W)],x_count:0 })
            || a.target_spec != TargetSpec::None
            || !crate::effect::squad_rallier_root_is_exact(&(a.effect)())
            || a.activation_zone != Zone::Battlefield
            || a.sorcery_speed_only
            || a.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
            || a.max_activations_per_turn.is_some()
            || a.face.is_some()
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    }
    canonical(id, bloom)?;
    Ok(bloom)
}
fn canonical(id: u16, bloom: bool) -> Result<Value> {
    if bloom {
        use crate::rules_meaning_bloom_tender_v1 as m;
        let selector = m::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        let value = m::meaning(&selector, id).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        m::validate(&value).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        serde_json::to_value(value).map_err(|e| Error::Projection(e.to_string()))
    } else {
        use crate::rules_meaning_squad_rallier_v1 as m;
        let selector = m::live_selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        let value = m::meaning(&selector, id, 0).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        m::validate(&value).map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        serde_json::to_value(value).map_err(|e| Error::Projection(e.to_string()))
    }
}
// Exact baseline routes serve actor-authorized references even when no family
// DTO marks this ID covered. Mandatory supplements remain independently issued.
pub(super) fn baseline(id: u16, d: &CardDef) -> Result<Value> {
    let bloom = validate_shape(id, d)?;
    Ok(json!({"card_id":id,"definition_owned_family_meanings":[canonical(id,bloom)?]}))
}
#[derive(serde::Serialize)]
enum CostRule {
    Mana(Cost),
    TapOriginalSource,
}
#[derive(serde::Serialize)]
enum Timing {
    WithPriorityAtInstantSpeed,
    CurrentManaWindowAndOrdinaryCreatureTapRestrictions,
}
#[derive(serde::Serialize)]
struct Ability {
    printed_ability_index: u8,
    activation_zone: Zone,
    current_source_controller_must_be_actor: bool,
    requires_active_effective_printed_slot: bool,
    timing: Timing,
    ordered_costs: Vec<CostRule>,
    target_count: u8,
    original_source_incarnation_bound_before_payment: bool,
    cost_committed_before_effect: bool,
    uses_stack: bool,
    countered_activation_never_refunds_payment: bool,
    canonical_effect: Value,
}
pub(in crate::current_mixed_policy_v1) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let bloom = validate_shape(id, d)?;
    let meaning = canonical(id, bloom)?;
    let ability = Ability {
        printed_ability_index: 0,
        activation_zone: Zone::Battlefield,
        current_source_controller_must_be_actor: true,
        requires_active_effective_printed_slot: true,
        timing: if bloom {
            Timing::CurrentManaWindowAndOrdinaryCreatureTapRestrictions
        } else {
            Timing::WithPriorityAtInstantSpeed
        },
        ordered_costs: if bloom {
            vec![CostRule::TapOriginalSource]
        } else {
            vec![CostRule::Mana(Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::W)],
                x_count: 0,
            })]
        },
        target_count: 0,
        original_source_incarnation_bound_before_payment: true,
        cost_committed_before_effect: true,
        uses_stack: !bloom,
        countered_activation_never_refunds_payment: !bloom,
        canonical_effect: meaning.clone(),
    };
    let characteristics = PermanentCharacteristics {
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: Vec::new(),
        colors: d.colors.to_vec(),
        mana_value: d.mana_value,
        power: d.power,
        toughness: d.toughness,
        ordered_keywords: Vec::new(),
        minimum_blockers: 1,
        token: false,
        land: false,
        enters_tapped: false,
        printed_mana_abilities: if bloom { 1 } else { 0 },
        printed_activated_abilities: if bloom { 0 } else { 1 },
    };
    let (activation, mana) = if bloom {
        (Vec::new(), vec![ability])
    } else {
        (vec![ability], Vec::new())
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,"canonical_family_roots":[meaning],
        "ordered_activation_roots":activation,"ordered_mana_roots":mana,
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {
            method:crate::state::CastMethodV4::Normal,ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(d.cost)],ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }]
    }})))
}
