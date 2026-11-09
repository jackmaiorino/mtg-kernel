//! Exact printed roots for the two Foundations Elf lords.
use super::{Error, Result};
use crate::{
    card_def::{
        CardDef, CardType, DynamicValueDef, Keywords, ManaAbilityAmountDef, ManaAbilityCostDef,
        ManaAbilityDef, Subtype, Supertype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef, PlayerRef},
    engine::{static_controlled_subtype_boost_for, StaticControlledSubtypeBoostDef},
    mana::{Cost, ManaColor, Pip},
    rl::{CardPublicV2, PlayerSeatV1, PublicObservationProjectionV2},
    state::Zone,
    trigger::{triggers_for, TriggerCondition},
};
use serde_json::{json, Value};

const DWYNEN: u16 = 210;
const ARCHDRUID: u16 = 375;

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Dwynen, Gilt-Leaf Daen" | "Elvish Archdruid")
}

/// The caller retains the common exclusions on other CardDef fields.
pub(super) fn printing(id: u16, def: &CardDef) -> Result<Option<Value>> {
    let (printing, generic, subtypes, supertypes, power, toughness, keywords, colors) =
        match def.name {
            "Dwynen, Gilt-Leaf Daen" => (
                DWYNEN,
                2,
                &[Subtype::Elf, Subtype::Warrior][..],
                &[Supertype::Legendary][..],
                3,
                4,
                Keywords::REACH,
                &[][..],
            ),
            "Elvish Archdruid" => (
                ARCHDRUID,
                1,
                &[Subtype::Elf, Subtype::Druid][..],
                &[][..],
                2,
                2,
                Keywords::NONE,
                &[ManaColor::G][..],
            ),
            _ => return Ok(None),
        };
    let cost = Cost {
        generic,
        pips: &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)],
        x_count: 0,
    };
    let trigger = triggers_for(id);
    let exact_roots = if id == DWYNEN {
        matches!(trigger, [trigger]
        if trigger.condition == TriggerCondition::Attacks
            && trigger.home_zone == Zone::Battlefield
            && trigger.face_index == 0
            && !trigger.intervening_if_kicked
            && !trigger.intervening_if_controls_another_source_card
            && (trigger.effect)() == (EffectOp::GainLifeByAttackingSubtypeCount {
                player: PlayerRef::Controller,
                subtype: Subtype::Elf,
            }))
            && def.mana_ability_def.is_none()
            && (def.mana_ability)().is_none()
    } else {
        trigger.is_empty()
            && def.mana_ability_def
                == Some(ManaAbilityDef {
                    cost: ManaAbilityCostDef::TapSelf,
                    amount: ManaAbilityAmountDef::Dynamic(
                        DynamicValueDef::ControlledPermanentsWithSubtype(Subtype::Elf),
                    ),
                    controller_damage: 0,
                    max_activations_per_turn: None,
                })
            && (def.mana_ability)()
                == Some(EffectOp::Sequence(vec![
                    EffectOp::TapObject {
                        object: ObjectRef::ThisSource,
                    },
                    EffectOp::AddMana {
                        player: PlayerRef::Controller,
                        colors: vec![ManaColor::G],
                    },
                ]))
    };
    if id != printing
        || crate::card_def::card_id_by_name(def.name) != Some(id)
        || def.cost != cost
        || def.mana_value != u16::from(generic) + 2
        || def.types != [CardType::Creature]
        || def.subtypes != subtypes
        || def.supertypes != supertypes
        || def.colors != [ManaColor::G]
        || def.power != Some(power)
        || def.toughness != Some(toughness)
        || def.keywords != keywords
        || def.is_token
        || def.is_land
        || def.target_spec != TargetSpec::None
        || !def.activated_abilities.is_empty()
        || def.produces_mana != colors
        || def.mana_ability_choices != colors
        || (def.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !matches!(
            static_controlled_subtype_boost_for(def.name),
            Some(StaticControlledSubtypeBoostDef {
                subtype: Subtype::Elf,
                exclude_source: true,
                power: 1,
                toughness: 1,
            })
        )
        || !exact_roots
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let attack = (id == DWYNEN).then(|| json!({
        "event":"ThisCreatureDeclaredAttacking", "home_zone":"Battlefield", "source_face":0,
        "controller":"CapturedControllerAtDeclaration", "mandatory":true, "uses_stack":true,
        "targets":[], "put_into_combat_is_not_declaration":true,
        "operation":"GainLifeEqualToCurrentAttackingPermanentsWithSubtype",
        "subtype":"Elf", "count_at":"Resolution", "controller_of_counted_objects":"CapturedTriggerController",
        "requires_current_battlefield_and_attacking_membership":true,
        "uses_current_effective_subtypes":true, "includes_this_source_if_still_eligible":true,
        "no_life_event_at_zero":true, "independent_of_original_source_survival":true,
        "normal_life_replacements_apply":true,
    }));
    let mana = (id == ARCHDRUID).then(|| json!({
        "operation":"AddManaPerCurrentControlledPermanentWithSubtype", "mana_color":"G", "subtype":"Elf",
        "cost":"TapThisPermanent", "controller":"CurrentSourceController", "uses_stack":false,
        "tap_cost_checks_summoning_sickness_if_currently_a_creature":true,
        "includes_this_source_if_still_eligible":true, "uses_current_effective_subtypes":true,
        "count_at":"ManaAbilityActivation", "normal_mana_modifiers_apply":true,
    }));
    Ok(Some(json!({"card_id":id,"elf_lord_printing":{
        "schema":"printed-elf-lord/v1", "cost":cost, "mana_value":def.mana_value,
        "types":def.types, "subtypes":def.subtypes,
        "supertypes":def.supertypes.iter().map(|&value| value as u8).collect::<Vec<_>>(),
        "colors":def.colors,
        "power":def.power, "toughness":def.toughness, "keywords":def.keywords.0, "token":false,
        "ordinary_cast":{"origin":"OwnHand", "timing":"OwnMainPhaseEmptyStackWithPriority",
            "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}], "targets":[],
            "resolution":"MoveOriginalSpellToBattlefieldUnderCasterControl",
            "normal_replacements_and_entry_triggers_apply":true,
            "other_cast_permissions_require_current_candidate_authority":true},
        "static_bonus":{"source_zone":"Battlefield", "source_face":0,
            "requires_active_printed_abilities_and_executable_definition":true,
            "recipient_type":"CurrentEffectiveCreature", "recipient_subtype":"Elf",
            "recipient_controller":"CurrentSourceController", "excludes_original_source":true,
            "power_delta":1, "toughness_delta":1, "layer":"7c", "places_no_counters":true,
            "does_not_target":true, "rechecks_current_control_type_and_subtype":true,
            "each_active_source_adds_independently":true},
        "legend_rule_applies":id==DWYNEN, "attack_trigger":attack, "mana_ability":mana,
    }})))
}

pub(in crate::current_mixed_policy_v1) fn visible_counts(
    board: &PublicObservationProjectionV2,
) -> Option<Value> {
    let battlefield = board.battlefield.iter().flatten();
    let named = |id| {
        matches!(id, DWYNEN | ARCHDRUID)
            && crate::card_def::CARD_DEFS
                .get(usize::from(id))
                .is_some_and(|def| handles(def.name))
    };
    if !battlefield
        .clone()
        .any(|card| named(card.stable.card_db_id))
        && !board.stack.iter().any(|item| named(item.source.card_db_id))
    {
        return None;
    }
    let elf = |card: &CardPublicV2| {
        Subtype::Elf.is_in_subtype_ids(&card.characteristics.effective_subtype_ids)
    };
    let counts = [PlayerSeatV1::P0, PlayerSeatV1::P1].map(|seat| {
        let controlled = battlefield.clone().filter(|card| {
            card.stable.controller == seat && card.stable.zone == Zone::Battlefield && elf(card)
        });
        let attacking = controlled
            .clone()
            .filter(|card| board.combat.ordered_attackers.contains(&card.stable));
        json!({"controller":seat,"current_elf_permanent_count":controlled.count(),
            "current_attacking_elf_permanent_count":attacking.count()})
    });
    Some(json!(counts))
}
