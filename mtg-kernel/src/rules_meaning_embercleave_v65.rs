//! Complete ordered Embercleave printing contract. No executable admission.
use crate::{
    card_def::{
        self, ActivationTargetFilter, CardCapability, CardDef, CardType, CostComponent,
        DynamicCountDef, GenericCostReductionDef, Keywords, Subtype, Supertype, TargetSpec,
        CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    optional_trigger_public_v1::SupertypeV1,
    state::Zone,
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "rules-meaning/embercleave/v65";
pub const NAME: &str = "Embercleave";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV65 {
    pub generic_mana: u8,
    pub colored_mana: Vec<ManaColor>,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<SupertypeV1>,
    pub colors: Vec<ManaColor>,
    pub power: Option<i16>,
    pub toughness: Option<i16>,
    pub keyword_bits: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV65 {
    FlashCast {
        requires_native_cast_permission_and_priority: bool,
        permits_nonmain_and_opponent_turn: bool,
        spell_targets: u8,
        pays_total_mana_and_normal_entry_replacements: bool,
    },
    ReduceOwnGenericCastCost {
        per_current_controlled_attacking_creature: u8,
        current_native_combat_roster_not_turn_declaration_history: bool,
        current_battlefield_controller_and_effective_creature_type: bool,
        generic_floor: u8,
        retained_red_pips: u8,
        printed_mana_value_unchanged: bool,
    },
    OnEntryAttach {
        mandatory_targets: u8,
        controlled_by_captured_trigger_controller: bool,
        no_legal_target_means_no_published_trigger: bool,
        response_window_after_target_selection: bool,
        original_source_and_target_incarnations_required: bool,
        source_control_change_does_not_reassign_trigger: bool,
        illegal_target_counters_trigger_without_attachment: bool,
    },
    EquippedCreature {
        power_delta: i16,
        toughness_delta: i16,
        keyword_bits: u32,
        both_players_turns: bool,
        source_printed_abilities_must_be_active: bool,
        host_keyword_removal_and_later_grants_follow_timestamps: bool,
    },
    Equip {
        generic_mana: u8,
        targets: u8,
        current_controlled_creature: bool,
        sorcery_speed_with_priority: bool,
        original_source_and_target_incarnations_required: bool,
        reattachment_removes_previous_host_grants: bool,
        source_departure_preserves_ability_but_prevents_attachment: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV65 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV65,
    pub ordered_rules: Vec<RuleV65>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let native = crate::rules_meaning_niv_mizzet_v63::producer_sha256();
        let mut hash = Sha256::new();
        for text in [
            SCHEMA,
            native.as_str(),
            include_str!("rules_meaning_embercleave_v65.rs"),
            include_str!("embercleave_public_v65.rs"),
            include_str!("embercleave_v1_tests.rs"),
            include_str!("lib.rs"),
        ] {
            hash.update((text.len() as u64).to_le_bytes());
            hash.update(text.as_bytes());
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub(crate) fn selected_id() -> Result<u16, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("V65 requires the actual pure Limited printing".into());
    }
    let id = card_def::card_id_by_name(NAME).ok_or("V65 Embercleave printing absent")?;
    exact_definition(id, &CARD_DEFS[usize::from(id)])?;
    Ok(id)
}
fn exact_definition(id: u16, d: &CardDef) -> Result<(), String> {
    if id != 462
        || !ordinary_foreign_fields_absent(d)
        || d.name != NAME
        || d.capability != CardCapability::Partial
        || d.cost.generic != 4
        || d.cost.x_count != 0
        || d.cost.pips != [Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)]
        || d.generic_cost_reduction
            != Some(GenericCostReductionDef {
                generic_per_count: 1,
                count: DynamicCountDef::ControllerAttackingCreatures,
            })
        || d.mana_value != 6
        || d.types != [CardType::Artifact]
        || d.subtypes != [Subtype::Equipment]
        || d.supertypes != [Supertype::Legendary]
        || d.colors != [ManaColor::R]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::FLASH
        || d.target_spec != TargetSpec::None
        || d.is_token
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err("V65 complete Embercleave printing changed".into());
    }
    let e = d.equipment.ok_or("V65 Equipment descriptor absent")?;
    if e.power_delta != 1
        || e.toughness_delta != 1
        || e.pt_controller_turn_only
        || e.add_subtype.is_some()
        || e.controller_turn_keywords != (Keywords::DOUBLE_STRIKE | Keywords::TRAMPLE)
        || e.other_turn_keywords != (Keywords::DOUBLE_STRIKE | Keywords::TRAMPLE)
        || e.noncreature_spell_damage_to_each_opponent != 0
        || e.job_select
        || e.granted_activated_ability.is_some()
    {
        return Err("V65 complete Equipment profile changed".into());
    }
    let [a] = d.activated_abilities else {
        return Err("V65 equip ability roster changed".into());
    };
    if !matches!(a.cost, [CostComponent::Mana(c)] if c.generic == 3 && c.x_count == 0 && c.pips.is_empty())
        || a.target_spec != TargetSpec::ControlledCreature
        || (a.effect)()
            != (EffectOp::AttachSourceToTarget {
                object: ObjectRef::Target(0),
            })
        || a.activation_zone != Zone::Battlefield
        || !a.sorcery_speed_only
        || a.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || a.max_activations_per_turn.is_some()
        || a.face.is_some()
    {
        return Err("V65 exact paid equip program changed".into());
    }
    let [t] = crate::trigger::triggers_for(id) else {
        return Err("V65 ETB roster changed".into());
    };
    if t.condition != TriggerCondition::Etb
        || t.home_zone != Zone::Battlefield
        || t.intervening_if_kicked
        || t.intervening_if_controls_another_source_card
        || t.face_index != 0
        || (t.effect)()
            != (EffectOp::AttachSourceToTarget {
                object: ObjectRef::Target(0),
            })
        || crate::trigger::target_spec_for_trigger(id, &(t.effect)())
            != Some(TargetSpec::ControlledCreature)
    {
        return Err("V65 exact native entry program changed".into());
    }
    Ok(())
}
pub fn printed_meaning() -> Result<MeaningV65, String> {
    let id = selected_id()?;
    let d = &CARD_DEFS[usize::from(id)];
    Ok(MeaningV65 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: id,
        printed: PrintedV65 {
            generic_mana: 4,
            colored_mana: vec![ManaColor::R, ManaColor::R],
            mana_value: 6,
            types: d.types.to_vec(),
            subtypes: d.subtypes.to_vec(),
            supertypes: vec![SupertypeV1::Legendary],
            colors: d.colors.to_vec(),
            power: None,
            toughness: None,
            keyword_bits: Keywords::FLASH.0,
        },
        ordered_rules: vec![
            RuleV65::FlashCast {
                requires_native_cast_permission_and_priority: true,
                permits_nonmain_and_opponent_turn: true,
                spell_targets: 0,
                pays_total_mana_and_normal_entry_replacements: true,
            },
            RuleV65::ReduceOwnGenericCastCost {
                per_current_controlled_attacking_creature: 1,
                current_native_combat_roster_not_turn_declaration_history: true,
                current_battlefield_controller_and_effective_creature_type: true,
                generic_floor: 0,
                retained_red_pips: 2,
                printed_mana_value_unchanged: true,
            },
            RuleV65::OnEntryAttach {
                mandatory_targets: 1,
                controlled_by_captured_trigger_controller: true,
                no_legal_target_means_no_published_trigger: true,
                response_window_after_target_selection: true,
                original_source_and_target_incarnations_required: true,
                source_control_change_does_not_reassign_trigger: true,
                illegal_target_counters_trigger_without_attachment: true,
            },
            RuleV65::EquippedCreature {
                power_delta: 1,
                toughness_delta: 1,
                keyword_bits: (Keywords::DOUBLE_STRIKE | Keywords::TRAMPLE).0,
                both_players_turns: true,
                source_printed_abilities_must_be_active: true,
                host_keyword_removal_and_later_grants_follow_timestamps: true,
            },
            RuleV65::Equip {
                generic_mana: 3,
                targets: 1,
                current_controlled_creature: true,
                sorcery_speed_with_priority: true,
                original_source_and_target_incarnations_required: true,
                reattachment_removes_previous_host_grants: true,
                source_departure_preserves_ability_but_prevents_attachment: true,
            },
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV65) -> Result<(), String> {
    if *value == printed_meaning()? {
        Ok(())
    } else {
        Err("V65 canonical complete meaning changed".into())
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
        && d.ward_cost.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.starting_loyalty.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.controlled_creature_grant.is_none()
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
