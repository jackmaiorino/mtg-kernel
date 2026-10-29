//! Exact native layer-7c team bonuses and their ordinary paid cast roots.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, ObjectRef},
    engine::{static_controlled_creature_boost_for_v1, StaticControlledCreatureFilterV1},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Anthem of Champions" | "Empyrean Eagle")
}

/// The caller retains every unrelated-field exclusion in baseline_recipe.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let eagle = d.name == "Empyrean Eagle";
    let cost = Cost {
        generic: u8::from(eagle),
        pips: if eagle {
            &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::U)]
        } else {
            &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::W)]
        },
        x_count: 0,
    };
    let boost =
        static_controlled_creature_boost_for_v1(d.name).ok_or(Error::UnsupportedPrintedRoot(id))?;
    let filter_matches = matches!(
        (&boost.filter, eagle),
        (StaticControlledCreatureFilterV1::All, false)
    ) || matches!((&boost.filter, eagle),
            (StaticControlledCreatureFilterV1::WithKeyword(k), true) if *k == Keywords::FLYING);
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if eagle { 3 } else { 2 }
        || d.types
            != if eagle {
                &[CardType::Creature][..]
            } else {
                &[CardType::Enchantment][..]
            }
        || d.subtypes
            != if eagle {
                &[Subtype::Bird, Subtype::Spirit][..]
            } else {
                &[][..]
            }
        || !d.supertypes.is_empty()
        || d.colors
            != if eagle {
                &[ManaColor::W, ManaColor::U][..]
            } else {
                &[ManaColor::W, ManaColor::G][..]
            }
        || d.power != eagle.then_some(2)
        || d.toughness != eagle.then_some(3)
        || d.keywords
            != if eagle {
                Keywords::FLYING
            } else {
                Keywords::NONE
            }
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !filter_matches
        || boost.exclude_source != eagle
        || boost.power != 1
        || boost.toughness != 1
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand",
        "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
        "targets":[],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"failed_or_countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let statics = json!([{"operation":"ModifyCurrentControlledCreatureSet","source_zone":"Battlefield","source_face":0,
        "requires_active_printed_abilities_and_executable_definition":true,"controller":"CurrentSourceController",
        "recipient_type":"CurrentEffectiveCreature","filter":if eagle {json!({"current_effective_keyword":"Flying"})} else {json!({"all":true})},
        "exclude_exact_source_incarnation":eagle,"power_delta":1,"toughness_delta":1,"layer":"7c",
        "places_no_counters":true,"does_not_target":true,"rechecks_current_control_type_and_layer6_keywords":true,
        "includes_later_entering_qualifying_creatures":true,"each_active_source_adds_independently":true,
        "ends_when_source_leaves_or_printed_abilities_are_inactive":true}]);
    Ok(Some(json!({"card_id":id,"team_static_printing":{
        "schema":"printed-live-team-boost/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,
        "subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":d.power,"toughness":d.toughness,
        "keywords":d.keywords.0,"token":false,"ordered_cast_roots":[cast],"ordered_static_roots":statics}})))
}
