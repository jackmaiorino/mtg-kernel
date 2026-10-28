//! Closed sacrifice-for-counter and land evasion activations.
use super::{Error, Result};
use crate::{
    card_def::{
        ActivationTargetFilter, CardDef, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Hungry Ghoul" | "Rogue's Passage")
}

/// Shared exclusions still reject every unrelated field on the definition.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let ghoul = d.name == "Hungry Ghoul";
    let cast_cost = if ghoul {
        Cost {
            generic: 1,
            pips: &[Pip::Colored(ManaColor::B)],
            x_count: 0,
        }
    } else {
        Cost::zero()
    };
    let payment = Cost {
        generic: if ghoul { 1 } else { 4 },
        pips: &[],
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cast_cost
        || d.mana_value != if ghoul { 2 } else { 0 }
        || d.types
            != if ghoul {
                &[CardType::Creature][..]
            } else {
                &[CardType::Land][..]
            }
        || d.subtypes
            != if ghoul {
                &[Subtype::Zombie][..]
            } else {
                &[][..]
            }
        || !d.supertypes.is_empty()
        || d.colors != if ghoul { &[ManaColor::B][..] } else { &[][..] }
        || d.power != if ghoul { Some(2) } else { None }
        || d.toughness != if ghoul { Some(2) } else { None }
        || d.is_land == ghoul
        || d.is_token
        || d.enters_battlefield_tapped
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || !crate::trigger::triggers_for(id).is_empty()
        || d.produces_mana != if ghoul { &[][..] } else { &[ManaColor::C][..] }
        || d.mana_ability_choices != d.produces_mana
        || (d.mana_ability)()
            != if ghoul {
                None
            } else {
                Some(EffectOp::Sequence(vec![
                    EffectOp::TapObject {
                        object: ObjectRef::ThisSource,
                    },
                    EffectOp::AddMana {
                        player: PlayerRef::Controller,
                        colors: vec![ManaColor::C],
                    },
                ]))
            }
        || (d.spell_effect)()
            != if ghoul {
                Some(EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                })
            } else {
                None
            }
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let [ability] = d.activated_abilities else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let costs = if ghoul {
        vec![
            CostComponent::Mana(payment),
            CostComponent::SacrificeOtherControlledCreatures(1),
        ]
    } else {
        vec![CostComponent::Mana(payment), CostComponent::Tap]
    };
    let program = if ghoul {
        EffectOp::AddPlusOneCounterToAbilitySource
    } else {
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::CANT_BE_BLOCKED,
        }
    };
    if ability.cost != costs
        || ability.target_spec
            != if ghoul {
                TargetSpec::None
            } else {
                TargetSpec::Creature
            }
        || ability.activation_zone != Zone::Battlefield
        || ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || (ability.effect)() != program
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let entry = if ghoul {
        json!({"kind":"Cast","method":"Normal","route":"Hand","origin":"OwnHand",
            "origin_owner_must_be_caster":true,"timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],
            "ordered_costs":[{"operation":"PayNativeAdjustedPrintedMana","printed_cost":cast_cost}],
            "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
            "countering_skips_entry_and_does_not_refund_cost":true,"failed_or_countered_spell_destination":"OwnersGraveyard",
            "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true})
    } else {
        json!({"kind":"PlayLand","origin":"OwnHand","origin_owner_must_be_player":true,
            "timing":"OwnMainPhaseEmptyStackWithPriority","uses_stack":false,"is_cast":false,
            "consumes_land_play_allowance":1,"entry_controller":"PlayerPlayingLand","enters_tapped":false,
            "normal_zone_replacements_and_entry_triggers_apply":true,"other_play_permissions_require_current_candidate_authority":true})
    };
    let mana = if ghoul {
        Value::Null
    } else {
        json!({"source_zone":"Battlefield","uses_stack":false,"ordered_costs":[{"operation":"TapOriginalSource","requires_untapped":true,
            "tap_cost_checks_readiness_if_currently_creature":true}],"ordered_resolution":[{"operation":"AddMana","color":"C","amount":1,"recipient":"ActivatingPlayer"}]})
    };
    let targets = if ghoul {
        json!([])
    } else {
        json!([{"slot":0,"minimum":1,"maximum":1,"chooser":"ActivatingPlayer",
            "filter":{"zone":"Battlefield","effective_type":"Creature","either_controller":true},
            "selected_before_cost_payment":true,"normal_target_legality_and_protection_apply":true,
            "rechecked_at_resolution":true,"binding":"OriginalSelectedBattlefieldIncarnation"}])
    };
    let activation_costs = if ghoul {
        json!([{"operation":"PayMana","cost":payment},{"operation":"SacrificeOtherControlledCreature","count":1,
            "payer":"ActivatingPlayer","source_itself_excluded":true,"tokens_and_tapped_creatures_allowed":true,
            "uses_current_effective_type_and_control":true,"is_cost_not_target_or_resolution_effect":true,
            "normal_zone_replacements_and_death_triggers_apply":true}])
    } else {
        json!([{"operation":"PayMana","cost":payment},{"operation":"TapOriginalSource","requires_untapped":true,
            "tap_cost_checks_readiness_if_currently_creature":true}])
    };
    let resolution = if ghoul {
        json!([{"operation":"PutPlusOnePlusOneCounterOnOriginalAbilitySource","count":1,
            "binding":"CapturedBattlefieldIncarnation","requires_same_source_still_battlefield_creature":true,
            "source_control_change_does_not_prevent_counter":true,"normal_counter_replacements_and_triggers_apply":true,
            "counter_has_no_cleanup_expiration":true,"departed_or_reentered_source_gets_no_counter":true}])
    } else {
        json!([{"operation":"GrantCantBeBlockedToOriginalLegalTarget","duration":"UntilCleanupExpirationAfterDiscard",
            "binds_original_battlefield_incarnation":true,"places_no_counter":true,"source_departure_does_not_end_effect":true,
            "does_not_remove_preexisting_blocks":true,"normal_continuous_layers_apply":true}])
    };
    Ok(Some(json!({"card_id":id,"utility_activation_printing":{
        "schema":"printed-utility-activation/v1","cost":cast_cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,
        "power":d.power,"toughness":d.toughness,"keywords":0,"token":false,
        "ordinary_entry":entry,"mana_ability":mana,
        "ordered_activated_roots":[{"ability_index":0,"source_zone":"Battlefield",
            "requires_active_printed_ability":true,"timing":"WithPriorityOnEitherPlayersTurn","uses_stack":true,
            "controller":"CapturedAbilityControllerAtActivation","targets":targets,"ordered_costs":activation_costs,
            "cost_choices_are_fixed_before_atomic_payment":true,"complete_cost_paid_before_stack_publication":true,
            "summoning_sickness_does_not_prohibit_symbol_free_activation":ghoul,"printed_activation_limit":null,
            "ordered_resolution":resolution,"source_departure_does_not_remove_published_ability":true,
            "failed_or_countered_ability_does_not_refund_cost":true}]
    }})))
}
