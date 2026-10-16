//! Closed Niv-Mizzet printed program. Source-only, no executable admission.
use crate::{
    card_def::{
        self, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
        CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "rules-meaning/niv-mizzet-visionary/v63";
pub const NAME: &str = "Niv-Mizzet, Visionary";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV63 {
    pub generic_mana: u8,
    pub colored_mana: Vec<ManaColor>,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<crate::optional_trigger_public_v1::SupertypeV1>,
    pub colors: Vec<ManaColor>,
    pub power: i16,
    pub toughness: i16,
    pub keywords: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV63 {
    Cast {
        active_player_main_phase_empty_stack_priority: bool,
        targets: u8,
        pay_printed_mana: bool,
        enters_under_original_spell_controller: bool,
        normal_zone_and_entry_replacements: bool,
        grants_additional_cast_permission: bool,
    },
    Flying,
    NoMaximumHandSize {
        current_controller: bool,
        while_on_battlefield_and_printed_abilities_active: bool,
        sampled_at_cleanup_discard_check: bool,
    },
    OnPositiveFinalNoncombatDamageToOpponent {
        any_source_controlled_at_damage_event: bool,
        exact_departure_controller_lki_if_source_left: bool,
        each_observer_captures_its_own_incarnation_and_controller: bool,
        post_replacement_and_prevention_amount_i32: bool,
        one_occurrence_per_source_player_damage_event: bool,
        not_combat_or_damage_to_permanents: bool,
    },
    DrawCapturedController {
        mandatory: bool,
        count_from_exact_committed_damage_receipt: bool,
        individual_native_draw_events: bool,
        no_targets_or_payment: bool,
        controller_changes_after_trigger_do_not_reassign: bool,
        source_or_observer_departure_does_not_cancel: bool,
        countered_trigger_draws_nothing: bool,
        atomic_no_public_answer_or_resumable_stage: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV63 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV63,
    pub ordered_rules: Vec<RuleV63>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        // Existing V61 source closure covers the shared native checked form/counter
        // dependencies. This is source binding only, never inherited permission.
        let inherited = crate::rules_meaning_graveyard_return_v61::producer_sha256();
        for text in [
            SCHEMA,
            inherited.as_str(),
            include_str!("rules_meaning_niv_mizzet_v63.rs"),
            include_str!("niv_mizzet_public_v63.rs"),
            include_str!("niv_mizzet_visionary_v1.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("source_type_hexproof_v1.rs"),
            include_str!("effect/crystal_prevention_v1.rs"),
            include_str!("effect/temporary_control_v1.rs"),
            include_str!("effect/temporary_control_v1/authority.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_statics_v1.rs"),
            include_str!("standard_legends_v1.rs"),
            include_str!("ids.rs"),
            include_str!("combat_damage_v1.rs"),
            include_str!("twinflame_tyrant_v1.rs"),
            include_str!("twinflame_tyrant_v1/combat.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("cauldron_grants_v1.rs"),
            include_str!("fishing_pole_v39.rs"),
            include_str!("heraldic_banner_v1.rs"),
            include_str!("banner_of_kinship_v1.rs"),
            include_str!("banner_of_kinship_v1/entry.rs"),
            include_str!("creature_types_v60.rs"),
            include_str!("starting_life_statics_v1.rs"),
            include_str!("standard_keywords_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("soul_shackled_zombie_v1.rs"),
        ] {
            h.update((text.len() as u64).to_le_bytes());
            h.update(text.as_bytes());
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub(crate) fn selected_id() -> Result<u16, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("V63 requires the actual pure Limited printing".into());
    }
    let id = card_def::card_id_by_name(NAME).ok_or("V63 Niv printing absent")?;
    let d = &CARD_DEFS[usize::from(id)];
    if id != 461
        || !ordinary_foreign_fields_absent(d)
        || d.name != NAME
        || d.capability != CardCapability::Partial
        || d.cost.generic != 4
        || d.cost.x_count != 0
        || d.cost.pips != [Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::R)]
        || d.mana_value != 6
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Dragon, Subtype::Wizard]
        || d.supertypes != [Supertype::Legendary]
        || d.colors != [ManaColor::U, ManaColor::R]
        || d.power != Some(5)
        || d.toughness != Some(5)
        || d.keywords != Keywords::FLYING
        || d.target_spec != TargetSpec::None
        || d.is_token
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !crate::trigger::triggers_for(id).is_empty()
    {
        return Err("V63 complete Niv definition or native program changed".into());
    }
    Ok(id)
}
pub fn printed_meaning() -> Result<MeaningV63, String> {
    let id = selected_id()?;
    let d = &CARD_DEFS[usize::from(id)];
    Ok(MeaningV63 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: id,
        printed: PrintedV63 {
            generic_mana: 4,
            colored_mana: vec![ManaColor::U, ManaColor::R],
            mana_value: 6,
            types: d.types.to_vec(),
            subtypes: d.subtypes.to_vec(),
            supertypes: vec![crate::optional_trigger_public_v1::SupertypeV1::Legendary],
            colors: d.colors.to_vec(),
            power: 5,
            toughness: 5,
            keywords: Keywords::FLYING.0,
        },
        ordered_rules: vec![
            RuleV63::Cast {
                active_player_main_phase_empty_stack_priority: true,
                targets: 0,
                pay_printed_mana: true,
                enters_under_original_spell_controller: true,
                normal_zone_and_entry_replacements: true,
                grants_additional_cast_permission: false,
            },
            RuleV63::Flying,
            RuleV63::NoMaximumHandSize {
                current_controller: true,
                while_on_battlefield_and_printed_abilities_active: true,
                sampled_at_cleanup_discard_check: true,
            },
            RuleV63::OnPositiveFinalNoncombatDamageToOpponent {
                any_source_controlled_at_damage_event: true,
                exact_departure_controller_lki_if_source_left: true,
                each_observer_captures_its_own_incarnation_and_controller: true,
                post_replacement_and_prevention_amount_i32: true,
                one_occurrence_per_source_player_damage_event: true,
                not_combat_or_damage_to_permanents: true,
            },
            RuleV63::DrawCapturedController {
                mandatory: true,
                count_from_exact_committed_damage_receipt: true,
                individual_native_draw_events: true,
                no_targets_or_payment: true,
                controller_changes_after_trigger_do_not_reassign: true,
                source_or_observer_departure_does_not_cancel: true,
                countered_trigger_draws_nothing: true,
                atomic_no_public_answer_or_resumable_stage: true,
            },
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV63) -> Result<(), String> {
    if *value == printed_meaning()? {
        Ok(())
    } else {
        Err("V63 canonical complete meaning changed".into())
    }
}
fn closed_fields(d: &CardDef) {
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
}
fn ordinary_foreign_fields_absent(d: &CardDef) -> bool {
    closed_fields(d);
    d.object_name == d.name
        && !d.is_land
        && d.generic_cost_reduction.is_none()
        && d.ward_cost.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.equipment.is_none()
        && d.starting_loyalty.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.controlled_creature_grant.is_none()
        && d.activated_abilities.is_empty()
        && (d.mana_ability)().is_none()
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.flashback.is_none()
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.kicked_target_spec.is_none()
        && d.escape.is_none()
        && d.mana_ability_choices.is_empty()
        && !d.enters_battlefield_tapped
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.enters_battlefield_tapped_unless.is_none()
        && d.attachment.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && d.optional_additional_cost.is_none()
        && !d.changeling
        && d.bestow.is_none()
        && !d.delve
        && d.adventure.is_none()
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.additional_mana_ability_conditions.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.creature_type_entry_v1.is_none()
        && d.other_controlled_nontoken_death_v1.is_none()
        && d.produces_mana.is_empty()
        && d.restricted_mana_abilities.is_empty()
        && d.token_counter_replacement_v48.is_none()
}
