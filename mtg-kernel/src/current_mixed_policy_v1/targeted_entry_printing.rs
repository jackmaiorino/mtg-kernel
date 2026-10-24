//! Closed targeted entry observers with owner-directed returns to hand.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, ObjectRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Mischievous Pup" | "Bigfin Bouncer" | "Elvish Regrower" | "Vampire Soulcaller"
    )
}

/// The caller still checks all unrelated CardDef fields in the common guard.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (cost, color, subtypes, power, toughness, keywords, target_spec, filter) = match d.name {
        "Mischievous Pup" => (
            Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::W)],
                x_count: 0,
            },
            ManaColor::W,
            &[Subtype::Dog][..],
            3,
            1,
            Keywords::FLASH,
            TargetSpec::UpToOneOtherControlledPermanent,
            json!({"zone":"Battlefield","relation":"ControlledByCapturedTriggerController","excludes_original_source_object":true}),
        ),
        "Bigfin Bouncer" => (
            Cost {
                generic: 3,
                pips: &[Pip::Colored(ManaColor::U)],
                x_count: 0,
            },
            ManaColor::U,
            &[Subtype::Shark, Subtype::Pirate][..],
            3,
            2,
            Keywords::NONE,
            TargetSpec::OpponentControlledCreature,
            json!({"zone":"Battlefield","type":"Creature","relation":"ControlledByOpponentOfCapturedTriggerController","uses_current_effective_type":true}),
        ),
        "Elvish Regrower" => (
            Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)],
                x_count: 0,
            },
            ManaColor::G,
            &[Subtype::Elf, Subtype::Druid][..],
            4,
            3,
            Keywords::NONE,
            TargetSpec::PermanentCardInOwnGraveyard,
            json!({"zone":"GraveyardOfCapturedTriggerController","kind":"Card","has_any_card_type":["Artifact","Creature","Enchantment","Land","Planeswalker"],"excludes_tokens_and_virtual_spell_copies":true}),
        ),
        "Vampire Soulcaller" => (
            Cost {
                generic: 4,
                pips: &[Pip::Colored(ManaColor::B)],
                x_count: 0,
            },
            ManaColor::B,
            &[Subtype::Vampire, Subtype::Warlock][..],
            3,
            2,
            Keywords::FLYING,
            TargetSpec::CreatureCardInOwnGraveyard,
            json!({"zone":"GraveyardOfCapturedTriggerController","kind":"Card","type":"Creature"}),
        ),
        _ => return Ok(None),
    };
    let pup = d.name == "Mischievous Pup";
    let program = if d.name == "Bigfin Bouncer" {
        EffectOp::MoveObject {
            object: ObjectRef::Target(0),
            to_zone: Zone::Hand,
        }
    } else {
        EffectOp::MoveAllTargets {
            to_zone: Zone::Hand,
        }
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(cost.generic) + cost.pips.len()
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors != [color]
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != keywords
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
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
    let [trigger] = crate::trigger::triggers_for(id) else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let TriggeredAbilityDef {
        condition,
        home_zone,
        face_index,
        intervening_if_kicked,
        intervening_if_controls_another_source_card,
        effect,
    } = trigger;
    if *condition != TriggerCondition::Etb
        || *home_zone != Zone::Battlefield
        || *face_index != 0
        || *intervening_if_kicked
        || *intervening_if_controls_another_source_card
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(target_spec)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":if pup {"InstantWithPriority"} else {"OwnMainPhaseEmptyStackWithPriority"},
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let target = json!({"slot":0,"kind":"Object","filter":filter,"minimum":if pup {0} else {1},"maximum":1,
        "chooser":"CapturedTriggerController","selected_at":"TriggerStackPlacement",
        "normal_target_legality_and_protection_apply":true,"rechecked_at_resolution":true,
        "binding":"OriginalSelectedObjectIncarnation"});
    let trigger = json!({"event":"ThisPermanentEntersBattlefield","home_zone":"Battlefield","source_face":0,
        "requires_active_printed_abilities_at_capture":true,"intervening_if":null,"mandatory":true,"uses_stack":true,
        "controller":"CapturedSourceControllerAtEntry","captures_original_source_incarnation":true,
        "survives_source_departure_and_later_control_change":true,"targets":[target],
        "zero_targets_allowed":pup,"zero_targets_perform_no_moves":pup,
        "required_target_absence_prevents_stack_placement":!pup,"all_chosen_targets_illegal_skips_resolution":true,
        "ordered_resolution":[{"operation":"ReturnEachLegalTargetToHand","recipient":"EachTargetsOwner",
            "requires_original_target_incarnation":true,"does_not_return_to_trigger_controllers_hand_unless_owner":true,
            "normal_zone_replacements_apply":true,"tokens_that_leave_the_battlefield_cease_normally":true}]});
    Ok(Some(json!({"card_id":id,"targeted_entry_printing":{
        "schema":"printed-targeted-entry/v1","cost":cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,
        "power":power,"toughness":toughness,"keywords":keywords.0,"token":false,
        "ordered_cast_roots":[cast],"ordered_triggers":[trigger]}})))
}
