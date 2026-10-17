//! Closed ordinary printings supplement their existing definition-owned family roots.
//! Descriptors grant no cast permission and contain no executable effect program.
use super::*;
use crate::card_def::{Keywords, Subtype};
use crate::mana::{Cost, ManaColor, Pip};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Banishing,
    Garruk,
    Hunter,
}
fn validate_shape(id: u16, d: &CardDef) -> Result<Kind> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || CARD_DEFS
        .get(usize::from(id))
        .is_none_or(|actual| actual.name != d.name)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let kind = match d.name {
        "Banishing Light" => Kind::Banishing,
        "Garruk's Uprising" => Kind::Garruk,
        "High-Society Hunter" => Kind::Hunter,
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    let hunter = kind == Kind::Hunter;
    let (cost, colors) = match kind {
        Kind::Banishing => (
            Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::W)],
                x_count: 0,
            },
            &[ManaColor::W][..],
        ),
        Kind::Garruk => (
            Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::G)],
                x_count: 0,
            },
            &[ManaColor::G][..],
        ),
        Kind::Hunter => (
            Cost {
                generic: 3,
                pips: &[Pip::Colored(ManaColor::B); 2],
                x_count: 0,
            },
            &[ManaColor::B][..],
        ),
    };
    let mana_value = if hunter { 5 } else { 3 };
    let subtypes = if hunter {
        &[Subtype::Vampire, Subtype::Noble][..]
    } else {
        &[][..]
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
        || d.types
            != if hunter {
                &[CardType::Creature][..]
            } else {
                &[CardType::Enchantment][..]
            }
        || d.subtypes != subtypes
        || d.colors != colors
        || !d.supertypes.is_empty()
        || d.power != if hunter { Some(5) } else { None }
        || d.toughness != if hunter { Some(3) } else { None }
        || d.keywords
            != if hunter {
                Keywords::FLYING
            } else {
                Keywords::NONE
            }
        || !d.activated_abilities.is_empty()
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
        || d.other_controlled_nontoken_death_v1.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    // The closed CardDef above authenticates all printing fields. Canonical
    // reconstruction additionally checks the actual ordered trigger programs.
    meanings(id, kind)?;
    Ok(kind)
}
fn meanings(id: u16, kind: Kind) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    macro_rules! roots { ($module:ident,$selector:ident,$meaning:ident,$validate:ident,[$($root:expr),+]) => {{
        use crate::$module as canonical;
        let selector = canonical::$selector().map_err(|_|Error::UnsupportedPrintedRoot(id))?;
        for root in [$($root),+] {
            let value = canonical::$meaning(&selector,id,root).map_err(|_|Error::UnsupportedPrintedRoot(id))?;
            canonical::$validate(&value).map_err(|_|Error::UnsupportedPrintedRoot(id))?;
            values.push(serde_json::to_value(value).map_err(|e|Error::Projection(e.to_string()))?);
        }
    }}; }
    match kind {
        Kind::Banishing => roots!(
            rules_meaning_banishing_v1,
            live_selector_v1,
            meaning_v1,
            validate_meaning_v1,
            [
                crate::rules_meaning_banishing_v1::RootV1::OrdinaryPermanentSpell,
                crate::rules_meaning_banishing_v1::RootV1::PrintedEtb { trigger_index: 0 },
                crate::rules_meaning_banishing_v1::RootV1::ImmediateUntilSourceLeavesReturn
            ]
        ),
        Kind::Garruk => roots!(
            rules_meaning_garruk_v1,
            live_selector_v1,
            meaning_v1,
            validate_meaning_v1,
            [
                crate::rules_meaning_garruk_v1::RootV1::OwnEtbInterveningFerociousDraw,
                crate::rules_meaning_garruk_v1::RootV1::ControlledCreaturesHaveTrample,
                crate::rules_meaning_garruk_v1::RootV1::ControlledCreaturePowerEntryDraw
            ]
        ),
        Kind::Hunter => roots!(
            rules_meaning_high_society_hunter_v1,
            live_selector,
            meaning,
            validate,
            [
                crate::rules_meaning_high_society_hunter_v1::RootV1::CreatureSpell,
                crate::rules_meaning_high_society_hunter_v1::RootV1::Attack,
                crate::rules_meaning_high_society_hunter_v1::RootV1::OtherNontokenDeath
            ]
        ),
    }
    Ok(values)
}
pub(super) fn baseline(id: u16, d: &CardDef) -> Result<Value> {
    let kind = validate_shape(id, d)?;
    Ok(json!({"card_id":id,"definition_owned_family_meanings":meanings(id,kind)?}))
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let kind = validate_shape(id, d)?;
    let characteristics = PermanentCharacteristics {
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: Vec::new(),
        colors: d.colors.to_vec(),
        mana_value: d.mana_value,
        power: d.power,
        toughness: d.toughness,
        ordered_keywords: if kind == Kind::Hunter {
            vec![PermanentKeyword::Flying]
        } else {
            Vec::new()
        },
        minimum_blockers: 1,
        token: false,
        land: false,
        enters_tapped: false,
        printed_mana_abilities: 0,
        printed_activated_abilities: 0,
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":characteristics,"canonical_family_roots":meanings(id,kind)?,
        "ordered_cast_roots":[OrdinaryPermanentCastRoot {
            method:crate::state::CastMethodV4::Normal,
            ordinary_route:crate::state::SpellCastRouteV4::Hand,ordinary_origin:Zone::Hand,
            timing:PermanentTiming::CasterMainPhaseEmptyStackWithPriority,targets:PermanentTargets::None,
            ordered_costs:[BaselineManaCost::PrintedMana(d.cost)],
            ordered_resolution:[PermanentEntry::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl],
            physical_departure:PermanentDeparture::BattlefieldOnResolutionOtherwiseGraveyard,
            other_cast_permissions_require_current_candidate_authority:true,zone_replacements_still_apply:true,
        }]
    }})))
}
#[cfg(test)]
#[path = "tests/enchantment_hunter_printing.rs"]
mod tests;
