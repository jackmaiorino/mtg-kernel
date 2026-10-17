//! Exact early-family printing supplements. No normalizer or executable authority.
use super::*;
use crate::card_def::{ActivationTargetFilter, CostComponent, Keywords, Subtype};
use crate::mana::{Cost, ManaColor, Pip};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ooze,
    Heartfire,
    Zombie,
    Cavalry,
}
fn validate_shape(id: u16, d: &CardDef) -> Result<Kind> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    let kind = match d.name {
        "Scavenging Ooze" => Kind::Ooze,
        "Heartfire Immolator" => Kind::Heartfire,
        "Soul-Shackled Zombie" => Kind::Zombie,
        "Axgard Cavalry" => Kind::Cavalry,
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    if CARD_DEFS
        .get(usize::from(id))
        .is_none_or(|actual| actual.name != d.name)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cost = Cost {
        generic: if kind == Kind::Zombie { 3 } else { 1 },
        pips: match kind {
            Kind::Ooze => &[Pip::Colored(ManaColor::G)],
            Kind::Heartfire | Kind::Cavalry => &[Pip::Colored(ManaColor::R)],
            Kind::Zombie => &[Pip::Colored(ManaColor::B)],
        },
        x_count: 0,
    };
    let mana_value = if kind == Kind::Zombie { 4 } else { 2 };
    let subtypes = match kind {
        Kind::Ooze => &[Subtype::Ooze][..],
        Kind::Heartfire => &[Subtype::Human, Subtype::Wizard][..],
        Kind::Zombie => &[Subtype::Zombie][..],
        Kind::Cavalry => &[Subtype::Dwarf, Subtype::Berserker][..],
    };
    let colors = match kind {
        Kind::Ooze => &[ManaColor::G][..],
        Kind::Heartfire | Kind::Cavalry => &[ManaColor::R][..],
        Kind::Zombie => &[ManaColor::B][..],
    };
    let power = if kind == Kind::Zombie { 4 } else { 2 };
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
        creature_type_entry_v1: _,
    } = d;
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != cost
        || d.mana_value != mana_value
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || d.colors != colors
        || !d.supertypes.is_empty()
        || d.power != Some(power)
        || d.toughness != Some(2)
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
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
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
        || d.mana_ability_def.is_some()
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
        || (d.other_controlled_nontoken_death_v1.is_some() || d.creature_type_entry_v1.is_some())
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    let triggers = crate::trigger::triggers_for(id);
    if kind == Kind::Zombie {
        if !d.activated_abilities.is_empty() || triggers.len() != 1 {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        crate::rules_meaning_soul_shackled_zombie_v1::root_meaning(id, 0)
            .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
    } else {
        if d.activated_abilities.len() != 1 {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        let ability = &d.activated_abilities[0];
        let mana = Cost {
            generic: 0,
            pips: if kind == Kind::Ooze {
                &[Pip::Colored(ManaColor::G)]
            } else {
                &[Pip::Colored(ManaColor::R)]
            },
            x_count: 0,
        };
        let correct_cost = match kind {
            Kind::Ooze => matches!(ability.cost,[CostComponent::Mana(actual)] if *actual==mana),
            Kind::Heartfire => {
                matches!(ability.cost,[CostComponent::Mana(actual),CostComponent::SacrificeSelf] if *actual==mana)
            }
            Kind::Cavalry => matches!(ability.cost, [CostComponent::Tap]),
            Kind::Zombie => unreachable!(),
        };
        let (target, effect) = match kind {
            Kind::Ooze => (
                TargetSpec::CardInGraveyards,
                EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature,
            ),
            Kind::Heartfire => (
                TargetSpec::CreatureOrPlaneswalker,
                EffectOp::StandardV1(
                    crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1,
                ),
            ),
            Kind::Cavalry => (
                TargetSpec::Creature,
                EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: ObjectRef::Target(0),
                    keyword: Keywords::HASTE,
                },
            ),
            Kind::Zombie => unreachable!(),
        };
        if !correct_cost
            || ability.target_spec != target
            || (ability.effect)() != effect
            || ability.activation_zone != Zone::Battlefield
            || ability.sorcery_speed_only
            || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
            || ability.max_activations_per_turn.is_some()
            || ability.face.is_some()
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        if kind == Kind::Cavalry {
            if !triggers.is_empty() {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
        } else if kind == Kind::Ooze {
            if !triggers.is_empty() {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            crate::rules_meaning_ooze_v1::root_meaning(id, 0)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
        } else {
            crate::rules_meaning_heartfire_v1::root_meaning(id, 0)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            let root = EffectOp::BindTemporaryBoostToTriggerSource {
                power: 1,
                toughness: 1,
            };
            if triggers.len() != 1
                || triggers[0].condition != crate::trigger::TriggerCondition::CastNoncreatureSpell
                || triggers[0].home_zone != Zone::Battlefield
                || triggers[0].face_index != 0
                || triggers[0].intervening_if_kicked
                || triggers[0].intervening_if_controls_another_source_card
                || (triggers[0].effect)() != root
                || crate::trigger::target_spec_for_trigger(id, &root) != Some(TargetSpec::None)
            {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
        }
    }
    Ok(kind)
}
#[derive(serde::Serialize)]
struct Prowess {
    printed_trigger_index: u8,
    trigger: NoncreatureCast,
    source_home_zone: Zone,
    active_effective_printed_ability_at_capture: bool,
    no_targets: bool,
    intervening_condition: bool,
    copied_spell_without_cast_does_not_trigger: bool,
    original_controller_captured: bool,
    later_control_or_ability_loss_does_not_cancel: bool,
    ordered_resolution: [ProwessBoost; 1],
}
#[derive(serde::Serialize)]
enum NoncreatureCast {
    CurrentControllerCastsNoncreatureSpell,
}
#[derive(serde::Serialize)]
struct ProwessBoost {
    power: i16,
    toughness: i16,
    original_source_battlefield_incarnation: bool,
    later_incarnation_does_not_receive_boost: bool,
    duration: BoostDuration,
}
#[derive(serde::Serialize)]
enum BoostDuration {
    NextCleanupIncludingRepeatedCleanup,
}
fn prowess() -> Prowess {
    Prowess {
        printed_trigger_index: 0,
        trigger: NoncreatureCast::CurrentControllerCastsNoncreatureSpell,
        source_home_zone: Zone::Battlefield,
        active_effective_printed_ability_at_capture: true,
        no_targets: true,
        intervening_condition: false,
        copied_spell_without_cast_does_not_trigger: true,
        original_controller_captured: true,
        later_control_or_ability_loss_does_not_cancel: true,
        ordered_resolution: [ProwessBoost {
            power: 1,
            toughness: 1,
            original_source_battlefield_incarnation: true,
            later_incarnation_does_not_receive_boost: true,
            duration: BoostDuration::NextCleanupIncludingRepeatedCleanup,
        }],
    }
}
#[derive(serde::Serialize)]
struct CavalryHaste {
    printed_ability_index: u8,
    target: ActivationTarget,
    either_controller: bool,
    target_incarnation_bound_at_announcement: bool,
    recheck_creature_target_legality_on_resolution: bool,
    illegal_only_target_counters_whole_ability: bool,
    ordered_resolution: [CavalryHasteInstruction; 1],
}
#[derive(serde::Serialize)]
enum CavalryHasteInstruction {
    GrantHasteToOriginalTargetIncarnation {
        duration: BoostDuration,
        later_target_incarnation_does_not_receive_grant: bool,
        source_departure_or_control_change_does_not_cancel: bool,
    },
}
fn cavalry_haste() -> CavalryHaste {
    CavalryHaste {
        printed_ability_index: 0,
        target: ActivationTarget::OneBattlefieldCreature,
        either_controller: true,
        target_incarnation_bound_at_announcement: true,
        recheck_creature_target_legality_on_resolution: true,
        illegal_only_target_counters_whole_ability: true,
        ordered_resolution: [
            CavalryHasteInstruction::GrantHasteToOriginalTargetIncarnation {
                duration: BoostDuration::NextCleanupIncludingRepeatedCleanup,
                later_target_incarnation_does_not_receive_grant: true,
                source_departure_or_control_change_does_not_cancel: true,
            },
        ],
    }
}
fn meanings(id: u16, kind: Kind) -> Result<Vec<Value>> {
    let convert = |value: std::result::Result<Value, serde_json::Error>| {
        value.map_err(|e| Error::Projection(e.to_string()))
    };
    Ok(match kind {
        Kind::Ooze => vec![convert(serde_json::to_value(
            crate::rules_meaning_ooze_v1::root_meaning(id, 0)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?,
        ))?],
        Kind::Heartfire => vec![
            json!({"card_id":id,"prowess":prowess()}),
            convert(serde_json::to_value(
                crate::rules_meaning_heartfire_v1::root_meaning(id, 0)
                    .map_err(|_| Error::UnsupportedPrintedRoot(id))?,
            ))?,
        ],
        Kind::Zombie => vec![convert(serde_json::to_value(
            crate::rules_meaning_soul_shackled_zombie_v1::root_meaning(id, 0)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?,
        ))?],
        Kind::Cavalry => vec![convert(serde_json::to_value(cavalry_haste()))?],
    })
}
// These exact full early DTOs use source_sha256, not root_sha256. Route only
// these authenticated recipes before the vanilla exclusion guard. The
// mandatory ordinary supplement is still independent of the covered-ID set.
pub(super) fn baseline(id: u16, d: &CardDef) -> Result<Value> {
    let kind = validate_shape(id, d)?;
    Ok(json!({"card_id":id,"definition_owned_family_meanings":meanings(id,kind)?}))
}
#[derive(serde::Serialize)]
enum ActivationCost {
    Mana(Cost),
    SacrificeOriginalSource,
    TapOriginalSource,
}
#[derive(serde::Serialize)]
enum ActivationTarget {
    OnePhysicalCardInEitherGraveyard,
    OneBattlefieldCreatureOrPlaneswalker,
    OneBattlefieldCreature,
}
#[derive(serde::Serialize)]
struct Activation {
    printed_ability_index: u8,
    activation_zone: Zone,
    timing: PrintedActivationTiming,
    current_source_controller_must_be_actor: bool,
    requires_active_effective_printed_slot_at_activation: bool,
    later_source_departure_or_ability_loss_does_not_counter: bool,
    target: ActivationTarget,
    original_source_incarnation_bound_before_payment: bool,
    target_announced_before_cost_payment: bool,
    ordered_costs: Vec<ActivationCost>,
    payment_precedes_stack_publication: bool,
    countered_activation_never_refunds_costs: bool,
    no_tap_cost: bool,
    no_summoning_sickness_restriction: bool,
    canonical_effect: Value,
}
pub(in crate::current_mixed_policy_v1) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let kind = validate_shape(id, d)?;
    let canonical = meanings(id, kind)?;
    let activation = if kind == Kind::Zombie {
        Vec::new()
    } else {
        let color = if kind == Kind::Ooze {
            ManaColor::G
        } else {
            ManaColor::R
        };
        let mut costs = if kind == Kind::Cavalry {
            vec![ActivationCost::TapOriginalSource]
        } else {
            vec![ActivationCost::Mana(Cost {
                generic: 0,
                pips: if color == ManaColor::G {
                    &[Pip::Colored(ManaColor::G)]
                } else {
                    &[Pip::Colored(ManaColor::R)]
                },
                x_count: 0,
            })]
        };
        if kind == Kind::Heartfire {
            costs.push(ActivationCost::SacrificeOriginalSource);
        }
        vec![Activation {
            printed_ability_index: 0,
            activation_zone: Zone::Battlefield,
            timing: PrintedActivationTiming::WithPriorityAtInstantSpeed,
            current_source_controller_must_be_actor: true,
            requires_active_effective_printed_slot_at_activation: true,
            later_source_departure_or_ability_loss_does_not_counter: true,
            target: match kind {
                Kind::Ooze => ActivationTarget::OnePhysicalCardInEitherGraveyard,
                Kind::Cavalry => ActivationTarget::OneBattlefieldCreature,
                Kind::Heartfire => ActivationTarget::OneBattlefieldCreatureOrPlaneswalker,
                Kind::Zombie => unreachable!(),
            },
            original_source_incarnation_bound_before_payment: true,
            target_announced_before_cost_payment: true,
            ordered_costs: costs,
            payment_precedes_stack_publication: true,
            countered_activation_never_refunds_costs: true,
            no_tap_cost: kind != Kind::Cavalry,
            no_summoning_sickness_restriction: kind != Kind::Cavalry,
            canonical_effect: canonical.last().unwrap().clone(),
        }]
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
        printed_mana_abilities: 0,
        printed_activated_abilities: d.activated_abilities.len() as u8,
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,"canonical_family_roots":canonical,"ordered_activation_roots":activation,
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {method:crate::state::CastMethodV4::Normal,
            ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(d.cost)],ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true}]
    }})))
}
