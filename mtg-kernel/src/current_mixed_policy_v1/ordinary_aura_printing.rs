//! Closed printed Auras with complete ordinary cast and continuous meanings.
use super::{Error, Result};
use crate::{
    card_def::{
        AttachmentDef, AuraCreatureStaticDef, CardDef, CardType, Keywords, Subtype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef},
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Blanchwood Armor" | "Twinblade Blessing")
}

/// The caller checks every unrelated CardDef field in the shared exclusion guard.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let armor = d.name == "Blanchwood Armor";
    let cost = Cost {
        generic: if armor { 2 } else { 1 },
        pips: if armor {
            &[Pip::Colored(ManaColor::G)]
        } else {
            &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)]
        },
        x_count: 0,
    };
    let grant = AuraCreatureStaticDef {
        power: if armor { 1 } else { 0 },
        toughness: if armor { 1 } else { 0 },
        keywords: if armor {
            Keywords::NONE
        } else {
            Keywords::DOUBLE_STRIKE
        },
        per_controlled_subtype: if armor { Some(Subtype::Forest) } else { None },
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 3
        || d.types != [CardType::Enchantment]
        || d.subtypes != [Subtype::Aura]
        || !d.supertypes.is_empty()
        || d.colors != [if armor { ManaColor::G } else { ManaColor::W }]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords
            != if armor {
                Keywords::NONE
            } else {
                Keywords::FLASH
            }
        || d.target_spec != TargetSpec::Creature
        || d.attachment != Some(AttachmentDef::AuraCreatureStatic(grant))
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::PutSourceOntoBattlefieldAttachedToTarget {
                target: ObjectRef::Target(0),
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(json!({"card_id":id,"ordinary_aura_printing":{
        "schema":"printed-creature-aura/v1","cost":cost,"mana_value":3,"types":d.types,
        "subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":null,"toughness":null,
        "keywords":d.keywords.0,"token":false,
        "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand",
            "timing":if armor {"OwnMainPhaseEmptyStackWithPriority"} else {"InstantWithPriority"},
            "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "target_slot":0,"target_filter":{"type":"Creature","zone":"Battlefield","controller":"EitherPlayer"},
            "captures_original_target_incarnation":true,"target_legality_rechecked_at_resolution":true,
            "ordered_resolution":[{"operation":"MoveOriginalAuraSpellToBattlefieldAttachedToOriginalTarget",
                "controller":"Caster","requires_original_target_still_legal":true}],
            "all_targets_illegal_skips_entire_resolution":true,"failed_spell_destination":"OwnersGraveyard",
            "countering_or_target_loss_does_not_refund_cost":true,
            "normal_zone_replacements_apply":true,"other_cast_permissions_require_current_candidate_authority":true}],
        "ordered_static_roots":[{"recipient":"CurrentExactEnchantedCreature","requires_active_printed_abilities":true,
            "power_delta":grant.power,"toughness_delta":grant.toughness,"keyword_bits":grant.keywords.0,
            "multiplier":if armor { json!({"operation":"CountCurrentControlledSubtype","controller":"AuraController","subtype":"Forest","tapped_status_irrelevant":true}) } else {json!({"operation":"Fixed","value":1})},
            "power_and_toughness_are_additive_modifiers":true,"places_no_counters":true,
            "normal_continuous_layers_and_timestamped_ability_removal_apply":true,
            "rechecked_from_current_aura_controller_attachment_and_battlefield":true,
            "ends_when_attachment_ends_or_source_leaves":true}],
        "attachment_legality":{"host":"Creature","normal_protection_and_state_based_actions_apply":true,
            "aura_without_legal_attached_host_goes_to_owners_graveyard":true}
    }})))
}
