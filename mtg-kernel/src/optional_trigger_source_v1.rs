//! Exact shared immutable closure. Public types contain no executable operands.
use crate::{
    card_def::{
        CardCapability, CardDef, CardType, Keywords, OtherControlledNontokenDeathDefV1, Subtype,
        TargetSpec, CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    state::Zone,
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Spinner,
    Quandary,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV1 {
    pub generic_mana: u8,
    pub colored_pips: Vec<ManaColor>,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub colors: Vec<ManaColor>,
    pub power: Option<i16>,
    pub toughness: Option<i16>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CastInstructionV1 {
    PayPrintedMana,
    MoveOriginalSpellFromStackToBattlefieldUnderCasterControl,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CastDepartureV1 {
    BattlefieldOnResolutionOtherwiseOwnersGraveyard,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub ordinary_hand_route: bool,
    pub caster_main_phase_empty_stack_priority: bool,
    pub untargeted: bool,
    pub ordered_instructions: Vec<CastInstructionV1>,
    pub physical_departure: CastDepartureV1,
    pub other_permissions_require_actual_candidate: bool,
    pub zone_replacements_apply: bool,
}
pub(crate) fn cast() -> CastV1 {
    CastV1 {
        ordinary_hand_route: true,
        caster_main_phase_empty_stack_priority: true,
        untargeted: true,
        ordered_instructions: vec![
            CastInstructionV1::PayPrintedMana,
            CastInstructionV1::MoveOriginalSpellFromStackToBattlefieldUnderCasterControl,
        ],
        physical_departure: CastDepartureV1::BattlefieldOnResolutionOtherwiseOwnersGraveyard,
        other_permissions_require_actual_candidate: true,
        zone_replacements_apply: true,
    }
}
pub(crate) fn digest(v: &impl Serialize) -> Result<String, String> {
    Ok(
        Sha256::digest(serde_json::to_vec(v).map_err(|e| e.to_string())?)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}
pub(crate) fn producer(schema: &str) -> String {
    let mut h = Sha256::new();
    for s in [
        schema,
        include_str!("optional_trigger_source_v1.rs"),
        include_str!("optional_trigger_public_v1.rs"),
        include_str!("rules_meaning_spinner_v54.rs"),
        include_str!("rules_meaning_quandary_v55.rs"),
        include_str!("spinner_public_v54.rs"),
        include_str!("quandary_public_v55.rs"),
        include_str!("card_def.rs"),
        include_str!("trigger.rs"),
        include_str!("creature_death_capture_v1.rs"),
        include_str!("spinner_of_souls_v1.rs"),
        include_str!("effect.rs"),
        include_str!("effect/discard_payment_v1.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("rl.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("planeswalker_v1.rs"),
        include_str!("heraldic_banner_v1.rs"),
        include_str!("starting_life_statics_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("controlled_creature_grants_v1.rs"),
        include_str!("../build.rs"),
        include_str!("counter_target_effects_v1.rs"),
        include_str!("zimone_paradox_sculptor_v1.rs"),
        include_str!("engine/omniscience_cast_v1.rs"),
        include_str!("resolution_cast_v1.rs"),
        include_str!("effect/harvester_v1.rs"),
        include_str!("leyline_axe_v1.rs"),
        include_str!("london_mulligan_v1.rs"),
        include_str!("life_loss_turn_v1.rs"),
        include_str!("soul_shackled_zombie_v1.rs"),
        include_str!("progenitus_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
        include_str!("effect/nonland_copy_v1.rs"),
        include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
        include_str!("effect/inspiration_resolution_v1.rs"),
        include_str!("effect/standard_exile.rs"),
        include_str!("snack_wide_life_v59.rs"),
        include_str!("snack_wide_life_v59/custody.rs"),
        include_str!("bloodthirsty_conqueror_v49.rs"),
        include_str!("fiendish_panda_v1.rs"),
        include_str!("fiendish_panda_v1/custody.rs"),
        include_str!("life_gain_turn_v1.rs"),
        include_str!("midnight_snack_v1.rs"),
    ] {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    }
    h.update(b"legacy-graveyard-return-guard/v61\0");
    h.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn printing(id: u16, kind: Kind) -> Result<&'static CardDef, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("pure Limited printing required".into());
    }
    let d = CARD_DEFS.get(usize::from(id)).ok_or("printing absent")?;
    // Exhaustive closure requires a deliberate decision for every successor field.
    let CardDef {
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
    let shape = match kind {
        Kind::Spinner => {
            d.name == "Spinner of Souls"
                && d.cost.generic == 2
                && d.cost.pips == [Pip::Colored(ManaColor::G)]
                && d.mana_value == 3
                && d.types == [CardType::Creature]
                && d.subtypes == [Subtype::Spider, Subtype::Spirit]
                && d.colors == [ManaColor::G]
                && d.power == Some(4)
                && d.toughness == Some(3)
                && d.keywords == Keywords::REACH
                && d.other_controlled_nontoken_death_v1
                    == Some(OtherControlledNontokenDeathDefV1::MayRevealCreatureToHandRandomBottom)
        }
        Kind::Quandary => {
            d.name == "Painful Quandary"
                && d.cost.generic == 3
                && d.cost.pips == [Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
                && d.mana_value == 5
                && d.types == [CardType::Enchantment]
                && d.subtypes.is_empty()
                && d.colors == [ManaColor::B]
                && d.power.is_none()
                && d.toughness.is_none()
                && d.keywords == Keywords::NONE
                && d.other_controlled_nontoken_death_v1.is_none()
        }
    };
    let valid = shape
        && d.object_name == d.name
        && matches!(d.capability, CardCapability::Partial | CardCapability::Full)
        && d.is_executable()
        && d.cost.x_count == 0
        && d.supertypes.is_empty()
        && !d.is_land
        && !d.is_token
        && d.target_spec == TargetSpec::None
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
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
        && d.produces_mana.is_empty()
        && d.mana_ability_choices.is_empty()
        && (d.mana_ability)().is_none()
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
        && d.mana_ability_def.is_none()
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
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.creature_type_entry_v1.is_none();
    if !valid {
        return Err("unsupported immutable printing fields".into());
    }
    let [a] = crate::trigger::triggers_for(id) else {
        return Err("exact one trigger required".into());
    };
    let condition = match kind {
        Kind::Spinner => TriggerCondition::OtherControlledNontokenCreatureDiesV1,
        Kind::Quandary => TriggerCondition::OpponentCastsSpellV1,
    };
    if a.condition != condition
        || a.home_zone != Zone::Battlefield
        || a.face_index != 0
        || a.intervening_if_kicked
        || a.intervening_if_controls_another_source_card
    {
        return Err("trigger shape changed".into());
    }
    Ok(d)
}
pub(crate) fn printed(d: &CardDef) -> PrintedV1 {
    PrintedV1 {
        generic_mana: d.cost.generic,
        colored_pips: d
            .cost
            .pips
            .iter()
            .map(|p| match p {
                Pip::Colored(c) => *c,
                _ => unreachable!("exact pips checked"),
            })
            .collect(),
        mana_value: d.mana_value,
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        colors: d.colors.to_vec(),
        power: d.power,
        toughness: d.toughness,
    }
}
