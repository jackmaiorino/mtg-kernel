//! Closed source-only rules for the two selected graveyard-return printings.
//! No observation, executable owner, model, sampler or deck admission is granted.
use crate::{
    card_def::{
        self, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
        CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    state::Zone,
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "rules-meaning/graveyard-return/v61";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FamilyV61 {
    Alesha,
    NineLives,
}
impl FamilyV61 {
    pub const ALL: [Self; 2] = [Self::Alesha, Self::NineLives];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Alesha => "Alesha, Who Laughs at Fate",
            Self::NineLives => "Nine-Lives Familiar",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV61 {
    pub generic_mana: u8,
    pub colored_mana: Vec<ManaColor>,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub legendary: bool,
    pub colors: Vec<ManaColor>,
    pub power: i16,
    pub toughness: i16,
    pub keyword_bits: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastEntryV61 {
    pub count: i32,
    pub cast_from_any_already_permitted_zone: bool,
    pub uncast_spell_copy_qualifies: bool,
    pub counters_before_entry_observers: bool,
    pub current_entry_replacements_apply_once: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV61 {
    PayPrintedMana,
    EnterUnderOriginalSpellController {
        active_player_main_phase_empty_stack_with_priority: bool,
        grants_additional_cast_permission: bool,
        revival_entry_if_genuinely_cast: Option<CastEntryV61>,
        countered_spell_goes_to_physical_owner_graveyard_subject_to_replacements: bool,
    },
    OnAttackPutCounterOnOriginalSource {
        plus_one: i32,
        exact_original_battlefield_incarnation: bool,
        counter_replacements_apply: bool,
    },
    OnCapturedControllerEndStepIfRaid {
        intervening_condition_at_collection_and_resolution: bool,
        actual_attack_declaration_this_turn_not_combat_damage: bool,
        one_target_creature_card_in_captured_controller_graveyard: bool,
        target_selection_mandatory_before_response_window: bool,
        captured_controller_may_decline_return_at_resolution_without_payment: bool,
        mana_value_at_most_original_source_current_power_or_departure_lki: bool,
        exact_announced_target_incarnation_rechecked_at_resolution: bool,
        return_under_physical_card_owner: bool,
        entry_replacements_apply: bool,
    },
    OnDeathWithPositiveDepartingRevival {
        ability_active_on_departing_effective_printing: bool,
        exact_departing_counter_lki: bool,
        departing_counter_condition_rechecked_at_death_trigger_resolution: bool,
        death_trigger_resolution_creates_delayed_trigger: bool,
        countered_death_trigger_creates_no_schedule: bool,
    },
    AtNextFutureAnyPlayerEndStep {
        consume_schedule_once_when_triggered: bool,
        already_begun_end_step_waits_for_next: bool,
        countered_delayed_trigger_does_not_retry: bool,
    },
    ReturnOriginalGraveyardIncarnation {
        captured_death_controller_not_physical_owner: bool,
        subtract_from_departure_count_before_entry_replacements: i32,
        current_entry_replacements_apply_once: bool,
        counters_before_entry_observers: bool,
        stale_graveyard_incarnation_is_no_op: bool,
        token_ceases_before_later_return: bool,
        return_is_not_cast: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV61 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub family: FamilyV61,
    pub card_id: u16,
    pub printed: PrintedV61,
    pub ordered_rules: Vec<InstructionV61>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for text in [
            SCHEMA,
            include_str!("rules_meaning_graveyard_return_v61.rs"),
            include_str!("graveyard_return_public_v61.rs"),
            include_str!("alesha_v1.rs"),
            include_str!("nine_lives_familiar_v1.rs"),
            include_str!("creature_death_capture_v1.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("effect/curator_v1.rs"),
            include_str!("effect/loot_v1.rs"),
            include_str!("effect/kykar_v1.rs"),
            include_str!("effect/greatwurm_v1.rs"),
            include_str!("effect/source_token_copy_v1.rs"),
            include_str!("engine/greatwurm_cast_v1.rs"),
            include_str!("effect/sphinx_lore_v1.rs"),
            include_str!("engine/sphinx_lore_cast_v1.rs"),
            include_str!("spell_costs_v1.rs"),
            include_str!("effect/squad_rallier_resolution_v1.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("state.rs"),
            include_str!("trigger.rs"),
            include_str!("card_def.rs"),
            include_str!("mana.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("../build.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("effect/standard_exile.rs"),
        ] {
            hash.update((text.len() as u64).to_le_bytes());
            hash.update(text.as_bytes());
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub(crate) fn selected_id(family: FamilyV61) -> Result<u16, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("V61 requires the actual pure Limited printings".into());
    }
    let id = card_def::card_id_by_name(family.name()).ok_or("V61 printing absent")?;
    if id
        != match family {
            FamilyV61::Alesha => 457,
            FamilyV61::NineLives => 458,
        }
    {
        return Err("V61 original printing ID changed".into());
    }
    let d = &CARD_DEFS[usize::from(id)];
    let (pips, subtypes, legendary, power, keywords) = match family {
        FamilyV61::Alesha => (
            vec![ManaColor::B, ManaColor::R],
            vec![Subtype::Human, Subtype::Warrior],
            true,
            2,
            Keywords::FIRST_STRIKE,
        ),
        FamilyV61::NineLives => (
            vec![ManaColor::B, ManaColor::B],
            vec![Subtype::Cat],
            false,
            1,
            Keywords::NONE,
        ),
    };
    if d.name != family.name()
        || d.capability != CardCapability::Partial
        || !ordinary_foreign_fields_absent(d)
        || d.cost.generic != 1
        || d.cost.x_count != 0
        || d.cost.pips != pips.iter().copied().map(Pip::Colored).collect::<Vec<_>>()
        || d.mana_value != 3
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || d.supertypes
            != if legendary {
                vec![Supertype::Legendary]
            } else {
                vec![]
            }
        || d.power != Some(power)
        || d.toughness != Some(power)
        || d.keywords != keywords
        || d.colors
            != if family == FamilyV61::Alesha {
                vec![ManaColor::B, ManaColor::R]
            } else {
                vec![ManaColor::B]
            }
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || d.is_token
    {
        return Err("V61 complete printing changed".into());
    }
    let triggers = crate::trigger::triggers_for(id);
    let expected: Vec<(TriggerCondition, EffectOp)> = match family {
        FamilyV61::Alesha => vec![
            (
                TriggerCondition::Attacks,
                crate::alesha_v1::attack_counter(),
            ),
            (
                TriggerCondition::BeginningControllerEndStepAfterControllerAttackedThisTurn,
                crate::alesha_v1::raid_return(),
            ),
        ],
        FamilyV61::NineLives => vec![(
            TriggerCondition::DiesWithRevivalCounterV1,
            crate::nine_lives_familiar_v1::death_trigger(),
        )],
    };
    if triggers.len() != expected.len()
        || triggers
            .iter()
            .zip(expected)
            .any(|(actual, (condition, effect))| {
                actual.condition != condition
                    || (actual.effect)() != effect
                    || actual.home_zone != Zone::Battlefield
                    || actual.face_index != 0
                    || actual.intervening_if_kicked
                    || actual.intervening_if_controls_another_source_card
            })
    {
        return Err("V61 native ordered trigger program changed".into());
    }
    Ok(id)
}
pub fn printed_meaning(family: FamilyV61) -> Result<MeaningV61, String> {
    let id = selected_id(family)?;
    let d = &CARD_DEFS[usize::from(id)];
    let mut ordered = vec![
        InstructionV61::PayPrintedMana,
        InstructionV61::EnterUnderOriginalSpellController {
            active_player_main_phase_empty_stack_with_priority: true,
            grants_additional_cast_permission: false,
            revival_entry_if_genuinely_cast: if family == FamilyV61::NineLives {
                Some(CastEntryV61 {
                    count: 8,
                    cast_from_any_already_permitted_zone: true,
                    uncast_spell_copy_qualifies: false,
                    counters_before_entry_observers: true,
                    current_entry_replacements_apply_once: true,
                })
            } else {
                None
            },
            countered_spell_goes_to_physical_owner_graveyard_subject_to_replacements: true,
        },
    ];
    match family {
        FamilyV61::Alesha => ordered.extend([
            InstructionV61::OnAttackPutCounterOnOriginalSource {
                plus_one: 1,
                exact_original_battlefield_incarnation: true,
                counter_replacements_apply: true,
            },
            InstructionV61::OnCapturedControllerEndStepIfRaid {
                intervening_condition_at_collection_and_resolution: true,
                actual_attack_declaration_this_turn_not_combat_damage: true,
                one_target_creature_card_in_captured_controller_graveyard: true,
                target_selection_mandatory_before_response_window: true,
                captured_controller_may_decline_return_at_resolution_without_payment: true,
                mana_value_at_most_original_source_current_power_or_departure_lki: true,
                exact_announced_target_incarnation_rechecked_at_resolution: true,
                return_under_physical_card_owner: true,
                entry_replacements_apply: true,
            },
        ]),
        FamilyV61::NineLives => ordered.extend([
            InstructionV61::OnDeathWithPositiveDepartingRevival {
                ability_active_on_departing_effective_printing: true,
                exact_departing_counter_lki: true,
                departing_counter_condition_rechecked_at_death_trigger_resolution: true,
                death_trigger_resolution_creates_delayed_trigger: true,
                countered_death_trigger_creates_no_schedule: true,
            },
            InstructionV61::AtNextFutureAnyPlayerEndStep {
                consume_schedule_once_when_triggered: true,
                already_begun_end_step_waits_for_next: true,
                countered_delayed_trigger_does_not_retry: true,
            },
            InstructionV61::ReturnOriginalGraveyardIncarnation {
                captured_death_controller_not_physical_owner: true,
                subtract_from_departure_count_before_entry_replacements: 1,
                current_entry_replacements_apply_once: true,
                counters_before_entry_observers: true,
                stale_graveyard_incarnation_is_no_op: true,
                token_ceases_before_later_return: true,
                return_is_not_cast: true,
            },
        ]),
    }
    Ok(MeaningV61 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        family,
        card_id: id,
        printed: PrintedV61 {
            generic_mana: d.cost.generic,
            colored_mana: d
                .cost
                .pips
                .iter()
                .map(|pip| match pip {
                    Pip::Colored(c) => Ok(*c),
                    _ => Err("V61 unexpected pip"),
                })
                .collect::<Result<_, _>>()?,
            mana_value: d.mana_value,
            types: d.types.to_vec(),
            subtypes: d.subtypes.to_vec(),
            legendary: d.supertypes == [Supertype::Legendary],
            colors: d.colors.to_vec(),
            power: d.power.ok_or("V61 power absent")?,
            toughness: d.toughness.ok_or("V61 toughness absent")?,
            keyword_bits: d.keywords.0,
        },
        ordered_rules: ordered,
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV61) -> Result<(), String> {
    if *value == printed_meaning(value.family)? {
        Ok(())
    } else {
        Err("V61 canonical complete meaning changed".into())
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
pub(crate) fn ordinary_foreign_fields_absent(d: &CardDef) -> bool {
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
