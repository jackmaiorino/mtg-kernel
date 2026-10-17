//! Complete typed printed program for the native Perforating Artist printing.
//! This source-only contract grants no ordinary consumer or deck admission.
use crate::{
    card_def::{self, CardCapability, CARD_DEFS},
    effect::{perforating_artist_v1 as native, EffectOp},
    rules_meaning_graveyard_return_v61::{ordinary_foreign_fields_absent, PrintedV61},
    state::Zone,
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "rules-meaning/perforating-artist/v64";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OpponentChoiceV64 {
    MaySacrificeOneControlledNonlandPermanent {
        is_target: bool,
        current_effective_land_type_excluded: bool,
        sacrifice_replacements_apply: bool,
        otherwise: Box<OpponentChoiceV64>,
    },
    MayDiscardOneCard {
        chooser_inspects_own_current_hand: bool,
        is_target: bool,
        discard_replacements_and_madness_apply: bool,
        otherwise: Box<OpponentChoiceV64>,
    },
    LoseLife {
        amount: i32,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV64 {
    PayPrintedMana,
    EnterUnderOriginalSpellController {
        main_phase_empty_stack_with_priority: bool,
        grants_additional_cast_permission: bool,
        countered_spell_uses_physical_owner_graveyard_subject_to_replacements: bool,
    },
    OnCapturedControllerEndStepIfRaid {
        actual_attack_declaration_this_turn_not_damage: bool,
        condition_rechecked_at_resolution: bool,
        source_need_not_have_attacked: bool,
        original_controller_survives_source_control_change_or_departure: bool,
        opponent_of_captured_controller_chooses: OpponentChoiceV64,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV64 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV61,
    pub ordered_rules: Vec<InstructionV64>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let predecessor = crate::rules_meaning_graveyard_return_v61::producer_sha256();
        let mut hash = Sha256::new();
        for text in [
            SCHEMA,
            predecessor.as_str(),
            include_str!("rules_meaning_artist_v64.rs"),
            include_str!("artist_public_v64.rs"),
            include_str!("effect/discard_payment_v1.rs"),
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
        return Err("V64 requires the actual pure Limited printing".into());
    }
    let id = card_def::card_id_by_name("Perforating Artist").ok_or("V64 printing absent")?;
    if id != 460 {
        return Err("V64 original printing ID changed".into());
    }
    let d = &CARD_DEFS[usize::from(id)];
    if d.capability != CardCapability::Partial
        || !ordinary_foreign_fields_absent(d)
        || !native::exact_definition(d)
    {
        return Err("V64 complete printing changed".into());
    }
    let rows = crate::trigger::triggers_for(id);
    if rows.len() != 1
        || rows[0].condition
            != TriggerCondition::BeginningControllerEndStepAfterControllerAttackedThisTurn
        || rows[0].home_zone != Zone::Battlefield
        || rows[0].face_index != 0
        || rows[0].intervening_if_kicked
        || rows[0].intervening_if_controls_another_source_card
        || (rows[0].effect)() != native::raid()
        || !matches!(native::raid(), EffectOp::Conditional { .. })
    {
        return Err("V64 ordered native Raid program changed".into());
    }
    Ok(id)
}
pub fn printed_meaning() -> Result<MeaningV64, String> {
    let id = selected_id()?;
    let d = &CARD_DEFS[usize::from(id)];
    Ok(MeaningV64 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: id,
        printed: PrintedV61 {
            generic_mana: d.cost.generic,
            colored_mana: d.colors.to_vec(),
            mana_value: d.mana_value,
            types: d.types.to_vec(),
            subtypes: d.subtypes.to_vec(),
            legendary: false,
            colors: d.colors.to_vec(),
            power: d.power.ok_or("V64 printed power absent")?,
            toughness: d.toughness.ok_or("V64 printed toughness absent")?,
            keyword_bits: d.keywords.0,
        },
        ordered_rules: vec![
            InstructionV64::PayPrintedMana,
            InstructionV64::EnterUnderOriginalSpellController {
                main_phase_empty_stack_with_priority: true,
                grants_additional_cast_permission: false,
                countered_spell_uses_physical_owner_graveyard_subject_to_replacements: true,
            },
            InstructionV64::OnCapturedControllerEndStepIfRaid {
                actual_attack_declaration_this_turn_not_damage: true,
                condition_rechecked_at_resolution: true,
                source_need_not_have_attacked: true,
                original_controller_survives_source_control_change_or_departure: true,
                opponent_of_captured_controller_chooses:
                    OpponentChoiceV64::MaySacrificeOneControlledNonlandPermanent {
                        is_target: false,
                        current_effective_land_type_excluded: true,
                        sacrifice_replacements_apply: true,
                        otherwise: Box::new(OpponentChoiceV64::MayDiscardOneCard {
                            chooser_inspects_own_current_hand: true,
                            is_target: false,
                            discard_replacements_and_madness_apply: true,
                            otherwise: Box::new(OpponentChoiceV64::LoseLife { amount: 3 }),
                        }),
                    },
            },
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV64) -> Result<(), String> {
    if *value == printed_meaning()? {
        Ok(())
    } else {
        Err("V64 canonical complete meaning changed".into())
    }
}
