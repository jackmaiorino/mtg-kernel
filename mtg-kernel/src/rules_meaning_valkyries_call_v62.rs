//! Closed printed meaning for native Partial Valkyrie's Call. No consumer admission.
use crate::{
    card_def::{self, CardCapability, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS},
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    state::Zone,
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "rules-meaning/valkyries-call/v62";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV62 {
    pub generic_mana: u8,
    pub colored_mana: Vec<ManaColor>,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub colors: Vec<ManaColor>,
    pub legendary: bool,
    pub power: Option<i16>,
    pub toughness: Option<i16>,
    pub keyword_bits: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV62 {
    PayPrintedMana,
    EnterUnderOriginalSpellController {
        active_player_main_phase_empty_stack_with_priority: bool,
        grants_additional_cast_permission: bool,
        uncast_permanent_copy_enters_as_token: bool,
        countered_spell_to_physical_owner_graveyard_subject_to_replacements: bool,
    },
    OnControlledNontokenNonAngelCreatureDeath {
        effective_creature_type_and_angel_subtype_before_departure: bool,
        printed_ability_active_on_original_source_before_departure: bool,
        source_need_not_be_a_different_object: bool,
        captured_controller_not_physical_owner: bool,
        capture_each_actual_simultaneous_death_before_collection: bool,
        source_departure_or_control_change_does_not_cancel_trigger: bool,
        mandatory_non_targeting_no_payment_or_optional_choice: bool,
        capture_collected_once: bool,
    },
    ReturnOriginalGraveyardIncarnation {
        under_physical_card_owner: bool,
        stale_incarnation_is_no_op: bool,
        no_creature_or_controller_recheck_on_graveyard_card: bool,
        return_is_not_cast: bool,
        countered_trigger_does_not_return_or_retry: bool,
    },
    InstallAtBattlefieldEntry {
        plus_one_before_replacements: i32,
        current_entry_and_counter_replacements_apply_once: bool,
        counters_before_entry_observers: bool,
        added_subtype: Subtype,
        preserves_other_types_and_subtypes: bool,
        grants_flying: bool,
        added_subtype_and_flying_bound_to_returned_incarnation: bool,
        flying_uses_native_ability_addition_removal_layers: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV62 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV62,
    pub ordered_rules: Vec<InstructionV62>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        // Preserve the actual inherited helper/native source closure, then bind
        // both new executable children and Call's definition-owned trigger.
        for text in [
            SCHEMA.to_owned(),
            crate::rules_meaning_graveyard_return_v61::producer_sha256(),
            include_str!("rules_meaning_valkyries_call_v62.rs").to_owned(),
            include_str!("valkyries_call_public_v62.rs").to_owned(),
            include_str!("valkyries_call_v1.rs").to_owned(),
            include_str!("standard_creatures_v1.rs").to_owned(),
            include_str!("planeswalker_v1.rs").to_owned(),
            include_str!("lib.rs").to_owned(),
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
        return Err("V62 requires the actual pure Limited printing".into());
    }
    let id = card_def::card_id_by_name("Valkyrie's Call").ok_or("V62 printing absent")?;
    if id != 459 {
        return Err("V62 original printing ID changed".into());
    }
    let d = &CARD_DEFS[usize::from(id)];
    if d.name != "Valkyrie's Call"
        || d.capability != CardCapability::Partial
        || !crate::rules_meaning_graveyard_return_v61::ordinary_foreign_fields_absent(d)
        || d.cost.generic != 3
        || d.cost.x_count != 0
        || d.cost.pips != [Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)]
        || d.mana_value != 5
        || d.types != [CardType::Enchantment]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::W]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || d.is_token
    {
        return Err("V62 complete printing changed".into());
    }
    let triggers = crate::trigger::triggers_for(id);
    if triggers.len() != 1
        || triggers[0].condition
            != TriggerCondition::ControlledNontokenCreatureDiesWithoutSubtypeV1(Subtype::Angel)
        || triggers[0].home_zone != Zone::Battlefield
        || triggers[0].face_index != 0
        || triggers[0].intervening_if_kicked
        || triggers[0].intervening_if_controls_another_source_card
        || (triggers[0].effect)() != EffectOp::BindValkyriesCallDeathV1
    {
        return Err("V62 native ordered trigger program changed".into());
    }
    Ok(id)
}
pub fn printed_meaning() -> Result<MeaningV62, String> {
    let id = selected_id()?;
    let d = &CARD_DEFS[usize::from(id)];
    Ok(MeaningV62 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: id,
        printed: PrintedV62 {
            generic_mana: d.cost.generic,
            colored_mana: vec![ManaColor::W, ManaColor::W],
            mana_value: d.mana_value,
            types: d.types.to_vec(),
            subtypes: d.subtypes.to_vec(),
            colors: d.colors.to_vec(),
            legendary: false,
            power: d.power,
            toughness: d.toughness,
            keyword_bits: d.keywords.0,
        },
        ordered_rules: vec![
            InstructionV62::PayPrintedMana,
            InstructionV62::EnterUnderOriginalSpellController {
                active_player_main_phase_empty_stack_with_priority: true,
                grants_additional_cast_permission: false,
                uncast_permanent_copy_enters_as_token: true,
                countered_spell_to_physical_owner_graveyard_subject_to_replacements: true,
            },
            InstructionV62::OnControlledNontokenNonAngelCreatureDeath {
                effective_creature_type_and_angel_subtype_before_departure: true,
                printed_ability_active_on_original_source_before_departure: true,
                source_need_not_be_a_different_object: true,
                captured_controller_not_physical_owner: true,
                capture_each_actual_simultaneous_death_before_collection: true,
                source_departure_or_control_change_does_not_cancel_trigger: true,
                mandatory_non_targeting_no_payment_or_optional_choice: true,
                capture_collected_once: true,
            },
            InstructionV62::ReturnOriginalGraveyardIncarnation {
                under_physical_card_owner: true,
                stale_incarnation_is_no_op: true,
                no_creature_or_controller_recheck_on_graveyard_card: true,
                return_is_not_cast: true,
                countered_trigger_does_not_return_or_retry: true,
            },
            InstructionV62::InstallAtBattlefieldEntry {
                plus_one_before_replacements: 1,
                current_entry_and_counter_replacements_apply_once: true,
                counters_before_entry_observers: true,
                added_subtype: Subtype::Angel,
                preserves_other_types_and_subtypes: true,
                grants_flying: true,
                added_subtype_and_flying_bound_to_returned_incarnation: true,
                flying_uses_native_ability_addition_removal_layers: true,
            },
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV62) -> Result<(), String> {
    if *value == printed_meaning()? {
        Ok(())
    } else {
        Err("V62 canonical complete meaning changed".into())
    }
}
