//! Printed sacrifice activations, including the departed Firebrand damage source.
use super::{Error, Result};
use crate::{
    card_def::{
        ActivationTargetFilter, CardDef, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Fanatical Firebrand" | "Cathar Commando")
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let firebrand = d.name == "Fanatical Firebrand";
    let cost = Cost {
        generic: if firebrand { 0 } else { 1 },
        pips: if firebrand {
            &[Pip::Colored(ManaColor::R)]
        } else {
            &[Pip::Colored(ManaColor::W)]
        },
        x_count: 0,
    };
    let mana = Cost {
        generic: 1,
        pips: &[],
        x_count: 0,
    };
    let subtypes = if firebrand {
        &[Subtype::Goblin, Subtype::Pirate][..]
    } else {
        &[Subtype::Human, Subtype::Soldier][..]
    };
    let keyword = if firebrand {
        Keywords::HASTE
    } else {
        Keywords::FLASH
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if firebrand { 1 } else { 2 }
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors
            != if firebrand {
                &[ManaColor::R][..]
            } else {
                &[ManaColor::W][..]
            }
        || d.power != Some(if firebrand { 1 } else { 3 })
        || d.toughness != Some(1)
        || d.keywords != keyword
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
    let expected_cost = if firebrand {
        vec![CostComponent::Tap, CostComponent::SacrificeSelf]
    } else {
        vec![CostComponent::Mana(mana), CostComponent::SacrificeSelf]
    };
    let program = if firebrand {
        EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 1,
        }
    } else {
        EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }
    };
    if ability.cost != expected_cost
        || (ability.effect)() != program
        || ability.target_spec
            != if firebrand {
                TargetSpec::AnyTarget
            } else {
                TargetSpec::ArtifactOrEnchantmentPermanent
            }
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":if firebrand {"OwnMainPhaseEmptyStackWithPriority"} else {"WithPriorityOnEitherPlayersTurn"},
        "flash_permission_requires_current_effective_flash":!firebrand,
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"normal_zone_replacements_and_entry_triggers_apply":true,
        "other_cast_permissions_require_current_candidate_authority":true});
    let costs = if firebrand {
        json!([{"operation":"TapOriginalSource","requires_untapped":true,"creature_tap_cost_requires_readiness_or_current_haste":true},{"operation":"SacrificeOriginalSource","normal_zone_replacements_apply":true}])
    } else {
        json!([{"operation":"PayMana","cost":mana},{"operation":"SacrificeOriginalSource","normal_zone_replacements_apply":true}])
    };
    let filter = if firebrand {
        json!({"kinds":["Player","BattlefieldCreature","BattlefieldPlaneswalker"],"either_controller":true,"requires_current_effective_type":true})
    } else {
        json!({"zone":"Battlefield","any_type":["Artifact","Enchantment"],"either_controller":true,"requires_current_effective_type":true})
    };
    let resolution = if firebrand {
        json!([{"operation":"DealDamage","amount":1,"recipient":"OriginalLegalTarget","damage_source":"CapturedOriginalSacrificedSource","uses_last_known_source_characteristics":true,"normal_damage_prevention_replacement_protection_and_lifelink_apply":true,"does_not_require_source_still_on_battlefield":true}])
    } else {
        json!([{"operation":"DestroyOriginalLegalTarget","normal_destruction_replacements_apply":true,"regenerate_if_shielded":true,"indestructible_prevents_destruction":true,"is_not_sacrifice_or_exile":true}])
    };
    let activation = json!({"ability_index":0,"source_zone":"Battlefield","requires_active_printed_ability":true,
        "timing":"WithPriorityOnEitherPlayersTurn","uses_stack":true,"controller":"CapturedAbilityControllerAtActivation",
        "targets":[{"slot":0,"filter":filter,"minimum":1,"maximum":1,"chooser":"ActivatingPlayer","selected_before_cost_payment":true,"normal_target_legality_and_protection_apply":true,"rechecked_at_resolution":true,"binding":"OriginalSelectedIncarnation"}],
        "ordered_costs":costs,"printed_activation_limit":null,"no_tap_cost":!firebrand,
        "summoning_sickness_does_not_prohibit_untapped_symbol_free_activation":!firebrand,
        "complete_cost_paid_before_stack_publication":true,"costs_are_not_refunded_when_target_disappears_or_ability_is_countered":true,
        "source_departure_does_not_remove_published_ability":true,"only_target_illegal_skips_entire_resolution":true,
        "ordered_resolution":resolution});
    Ok(Some(
        json!({"card_id":id,"sacrifice_activation_printing":{"schema":"printed-sacrifice-activation/v1",
        "cost":cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,
        "power":d.power,"toughness":1,"keywords":keyword.0,"token":false,
        "ordered_cast_roots":[cast],"ordered_activated_roots":[activation]}}),
    ))
}
