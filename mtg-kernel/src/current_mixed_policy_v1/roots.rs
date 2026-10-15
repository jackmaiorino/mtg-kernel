//! Exact small baseline printing descriptors. Other missing typed roots refuse.
use super::{Error, Result};
use crate::card_def::{CardCapability, CardType, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef, PlayerRef};
use crate::state::Zone;
use serde_json::{json, Value};

pub(super) fn baseline(id: u16) -> Result<Value> {
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    let treasure = d.name == "Treasure Token"
        && d.is_token
        && d.types == [CardType::Artifact]
        && d.subtypes == [crate::card_def::Subtype::Treasure]
        && d.colors.is_empty()
        && d.supertypes.is_empty()
        && !d.is_land
        && d.mana_value == 0
        && d.produces_mana == d.mana_ability_choices
        && d.produces_mana
            == [
                crate::mana::ManaColor::W,
                crate::mana::ManaColor::U,
                crate::mana::ManaColor::B,
                crate::mana::ManaColor::R,
                crate::mana::ManaColor::G,
            ]
        && d.power.is_none()
        && d.toughness.is_none()
        && d.cost == crate::mana::Cost::zero()
        && d.target_spec == TargetSpec::None
        && d.keywords == crate::card_def::Keywords::NONE
        && (d.spell_effect)().is_none()
        && (d.mana_ability)().is_none()
        && d.mana_ability_def
            == Some(crate::card_def::ManaAbilityDef {
                cost: crate::card_def::ManaAbilityCostDef::TapAndSacrificeSelf,
                amount: crate::card_def::ManaAbilityAmountDef::Fixed(1),
                controller_damage: 0,
                max_activations_per_turn: None,
            });
    if !(d.capability == CardCapability::Full
        && d.object_name == d.name
        && d.activated_abilities.is_empty()
        && d.ward_cost.is_none()
        && d.alt_cost.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.flashback.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.optional_additional_cost.is_none()
        && d.equipment.is_none()
        && d.attachment.is_none()
        && d.starting_loyalty.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_creature_grant.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.generic_cost_reduction.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.kicked_target_spec.is_none()
        && d.escape.is_none()
        && !d.enters_battlefield_tapped
        && (d.mana_ability_def.is_none() || treasure)
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.enters_battlefield_tapped_unless.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && !d.changeling
        && d.bestow.is_none()
        && !d.delve
        && d.adventure.is_none()
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.additional_mana_ability_conditions.is_empty()
        && d.restricted_mana_abilities.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.token_counter_replacement_v48.is_none()
        && crate::trigger::triggers_for(id).is_empty())
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    if treasure {
        return Ok(
            json!({"card_id":id,"primitive_printing":{"token":true,"types":["Artifact"],"subtypes":["Treasure"],
            "ordered_mana_costs":["TapSelf","SacrificeSelf"],"mana_recipient":"Controller","mana_amount":1,"color_choices":d.mana_ability_choices}}),
        );
    }
    let mana = match (d.mana_ability)() {
        None if d.produces_mana.is_empty() && d.mana_ability_choices.is_empty() => None,
        Some(EffectOp::Sequence(ops))
            if ops
                == vec![
                    EffectOp::TapObject {
                        object: ObjectRef::ThisSource,
                    },
                    EffectOp::AddMana {
                        player: PlayerRef::Controller,
                        colors: d.produces_mana.to_vec(),
                    },
                ]
                && d.produces_mana.len() == 1
                && d.mana_ability_choices == d.produces_mana =>
        {
            Some(d.produces_mana.to_vec())
        }
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    let instruction = match (d.spell_effect)() {
        None if mana.is_none()
            && (d.is_token && d.types == [CardType::Creature]
                || d.name == "Face-down creature"
                || d.name == "Face-down card") =>
        {
            json!({"operation":"PublicTokenOrMaskedCharacteristics","not_cast_permission":true})
        }
        None if d.is_land
            && d.types == [CardType::Land]
            && mana.is_some()
            && d.cost == crate::mana::Cost::zero()
            && d.target_spec == TargetSpec::None
            && d.power.is_none()
            && d.toughness.is_none() =>
        {
            json!({"operation":"PlayLand"})
        }
        Some(EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        }) if !d.is_land
            && d.types == [CardType::Creature]
            && d.target_spec == TargetSpec::None
            && d.power.is_some()
            && d.toughness.is_some()
            && (mana.is_none() || d.name == "Llanowar Elves") =>
        {
            json!({"operation":"EnterBattlefield","controller":"Caster"})
        }
        Some(EffectOp::DealDamage {
            target: crate::effect::TargetRef::Target(0),
            amount: 3,
        }) if d.name == "Lightning Bolt"
            && d.types == [CardType::Instant]
            && d.target_spec == TargetSpec::AnyTarget
            && !d.is_land
            && mana.is_none()
            && d.power.is_none()
            && d.toughness.is_none()
            && d.keywords == crate::card_def::Keywords::NONE =>
        {
            json!({"operation":"Damage","target_slot":0,"amount":3,"recipient_union":["Player","Creature","Planeswalker"]})
        }
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    Ok(json!({"card_id":id,"primitive_printing":{
        "cost":d.cost,"types":d.types.iter().map(|v|*v as u8).collect::<Vec<_>>(),
        "subtypes":d.subtypes.iter().map(|v|*v as u16).collect::<Vec<_>>(),
        "supertypes":d.supertypes.iter().map(|v|*v as u8).collect::<Vec<_>>(),
        "colors":d.colors,"power":d.power,"toughness":d.toughness,
        "keywords":d.keywords.0,"mana_value":d.mana_value,"token":d.is_token,
        "ordered_instructions":[instruction],
        "mana_ability":mana.map(|colors|json!({"cost":"TapSelf","recipient":"Controller","colors":colors,"amount":1}))
    }}))
}

pub(super) fn expand_thin(record: &serde_json::Map<String, Value>) -> Result<Value> {
    let id = record
        .get("card_id")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .ok_or(Error::Range)?;
    let root = record.get("root");
    macro_rules! typed {
        ($module:ident,$selector:ident,$function:ident,$argument:expr) => {{
            let selector =
                crate::$module::$selector().map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            let meaning = crate::$module::$function(&selector, id, $argument)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            serde_json::to_value(meaning).map_err(|e| Error::Projection(e.to_string()))
        }};
    }
    match CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?
        .name
    {
        "Sower of Chaos" => typed!(
            rules_meaning_sower_v1,
            live_selector_v1,
            meaning_v1,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Wildwood Scourge" => {
            let selector = crate::rules_meaning_wildwood_v1::live_selector_v1()
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            let meaning = crate::rules_meaning_wildwood_v1::meaning_v1(&selector, id)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?;
            serde_json::to_value(meaning).map_err(|e| Error::Projection(e.to_string()))
        }
        "Frenzied Goblin" => typed!(
            rules_meaning_frenzied_goblin_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Authority of the Consuls" => typed!(
            rules_meaning_authority_v1,
            live_selector_v1,
            meaning_v1,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Involuntary Employment" => typed!(
            rules_meaning_employment_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Slumbering Cerberus" => typed!(
            rules_meaning_cerberus_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Mild-Mannered Librarian" => typed!(
            rules_meaning_librarian_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Temporal Manipulation" => typed!(
            rules_meaning_extra_turn_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Juggernaut" => typed!(
            rules_meaning_juggernaut_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Ashroot Animist" => typed!(
            rules_meaning_ashroot_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Banishing Light" => typed!(
            rules_meaning_banishing_v1,
            live_selector_v1,
            meaning_v1,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Stromkirk Bloodthief" => typed!(
            rules_meaning_stromkirk_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Vampire Gourmand" => typed!(
            rules_meaning_gourmand_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Garruk's Uprising" => typed!(
            rules_meaning_garruk_v1,
            live_selector_v1,
            meaning_v1,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Crystal Barricade" => typed!(
            rules_meaning_crystal_barricade_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Arahbo, the First Fang" => typed!(
            rules_meaning_arahbo_v1,
            live_selector_v1,
            meaning_v1,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Strongbox Raider" => typed!(
            rules_meaning_strongbox_v1,
            live_selector,
            meaning,
            record
                .get("ability_index")
                .or_else(|| record.get("trigger_index"))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or(Error::UnsupportedPrintedRoot(id))?
        ),
        "Krenko, Mob Boss" => typed!(
            rules_meaning_krenko_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Zul Ashur, Lich Lord" => typed!(
            rules_meaning_zul_ashur_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Heraldic Banner" => typed!(
            rules_meaning_heraldic_banner_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Fishing Pole" => typed!(
            rules_meaning_fishing_pole_v39,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "High-Society Hunter" => typed!(
            rules_meaning_high_society_hunter_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        "Akroma's Memorial" => typed!(
            rules_meaning_akroma_v1,
            live_selector,
            meaning,
            serde_json::from_value(root.cloned().ok_or(Error::UnsupportedPrintedRoot(id))?)
                .map_err(|_| Error::UnsupportedPrintedRoot(id))?
        ),
        _ => Err(Error::UnsupportedPrintedRoot(id)),
    }
}
