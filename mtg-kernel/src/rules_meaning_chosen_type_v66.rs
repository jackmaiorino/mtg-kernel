//! Closed printed meanings for native Partial Banner and Courtyard, without admission.
use crate::{
    banner_of_kinship_v1::entry,
    card_def::{self, CardType, Subtype, Supertype, CARD_DEFS},
    creature_types_v60::CreatureTypeV60,
    mana::ManaColor,
    optional_trigger_public_v1::SupertypeV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "rules-meaning/chosen-creature-type/v66";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FamilyV66 {
    BannerOfKinship,
    SecludedCourtyard,
}
impl FamilyV66 {
    pub const fn card_id(self) -> u16 {
        match self {
            Self::BannerOfKinship => 463,
            Self::SecludedCourtyard => 470,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV66 {
    PayFiveAndCastArtifactUnderOrdinaryNativePermission,
    PlayLandUnderOrdinaryNativePermission,
    AsEntersChooseCreatureType {
        domain: Vec<CreatureTypeV60>,
        original_entry_controller_chooses: bool,
        simultaneous_choices_in_native_apnap_order_before_entries_or_observers: bool,
        new_incarnation_and_token_copy_choose_again: bool,
        choice_not_copied_and_not_changed_by_control_change: bool,
    },
    EnterWithFellowship {
        count_preentry_effective_controlled_creatures_of_chosen_type: bool,
        excludes_other_simultaneous_entrants: bool,
        captured_preentry_counter_replacements_apply_once: bool,
        counters_installed_before_entry_observers: bool,
        checked_i32_storage_no_saturation: bool,
    },
    ChosenTypeAnthem {
        effective_current_creatures_of_current_controller: bool,
        current_fellowship_is_both_power_and_toughness_bonus: bool,
        native_layer_7c_and_printed_ability_suppression: bool,
        source_departure_ends_bonus: bool,
    },
    TapForUnrestrictedColorless {
        amount: u8,
        colors: Vec<ManaColor>,
        native_mana_ability_no_stack: bool,
    },
    TapForRestrictedColor {
        amount: u8,
        alternatives: Vec<ManaColor>,
        creature_spell_must_have_chosen_effective_type: bool,
        creature_or_creature_card_activation_must_have_chosen_effective_type: bool,
        includes_hand_and_graveyard_creature_card_abilities: bool,
        changeling_matches_every_domain_type: bool,
        cannot_pay_unrelated_spell_or_noncreature_ability: bool,
        float_retains_producing_incarnation_and_choice_after_departure_control_or_reentry: bool,
        mana_clears_at_native_step_phase_boundary: bool,
        native_mana_ability_no_stack: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV66 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub family: FamilyV66,
    pub card_id: u16,
    pub generic_mana: u8,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub colors: Vec<ManaColor>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<SupertypeV1>,
    pub power: Option<i16>,
    pub toughness: Option<i16>,
    pub keyword_bits: u32,
    pub is_land: bool,
    pub ordered_rules: Vec<InstructionV66>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        // The inherited closure binds native engine/event/state/effect/mana,
        // checked characteristics, counter replacements and generated definitions.
        // Bind the actually executed entry and domain children explicitly too.
        for text in [
            SCHEMA.to_owned(),
            crate::rules_meaning_valkyries_call_v62::producer_sha256(),
            crate::rules_meaning_next_five_v59::producer_sha256(),
            include_str!("rules_meaning_chosen_type_v66.rs").to_owned(),
            include_str!("chosen_type_public_v66.rs").to_owned(),
            include_str!("banner_of_kinship_v1.rs").to_owned(),
            include_str!("banner_of_kinship_v1/entry.rs").to_owned(),
            include_str!("linked_aura_operation_v1.rs").to_owned(),
            include_str!("heraldic_banner_v1.rs").to_owned(),
            include_str!("linked_aura_return_v1.rs").to_owned(),
            include_str!("season_token_attachment_v48.rs").to_owned(),
            include_str!("fake_your_own_death_v50.rs").to_owned(),
            include_str!("fake_your_own_death_v50/return_aura.rs").to_owned(),
            include_str!("fake_your_own_death_v50/spell_custody.rs").to_owned(),
            include_str!("creature_types_v60.rs").to_owned(),
            crate::creature_types_v60::DOMAIN_INPUT_V60.to_owned(),
            include_str!("../build_standard_v1.rs").to_owned(),
            include_str!("lib.rs").to_owned(),
        ] {
            hash.update((text.len() as u64).to_le_bytes());
            hash.update(text.as_bytes());
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub(crate) fn selected(family: FamilyV66) -> Result<u16, String> {
    let id = family.card_id();
    let exact = match family {
        FamilyV66::BannerOfKinship => entry::owns_definition(id),
        FamilyV66::SecludedCourtyard => entry::owns_courtyard(id),
    };
    if !exact {
        return Err("V66 requires the exact closed native Partial printing".into());
    }
    Ok(id)
}
pub fn printed_meaning(family: FamilyV66) -> Result<MeaningV66, String> {
    let id = selected(family)?;
    let d = &CARD_DEFS[usize::from(id)];
    let mut ordered_rules = vec![match family {
        FamilyV66::BannerOfKinship => {
            InstructionV66::PayFiveAndCastArtifactUnderOrdinaryNativePermission
        }
        FamilyV66::SecludedCourtyard => InstructionV66::PlayLandUnderOrdinaryNativePermission,
    }];
    ordered_rules.push(InstructionV66::AsEntersChooseCreatureType {
        domain: CreatureTypeV60::ALL.to_vec(),
        original_entry_controller_chooses: true,
        simultaneous_choices_in_native_apnap_order_before_entries_or_observers: true,
        new_incarnation_and_token_copy_choose_again: true,
        choice_not_copied_and_not_changed_by_control_change: true,
    });
    match family {
        FamilyV66::BannerOfKinship => {
            ordered_rules.push(InstructionV66::EnterWithFellowship {
                count_preentry_effective_controlled_creatures_of_chosen_type: true,
                excludes_other_simultaneous_entrants: true,
                captured_preentry_counter_replacements_apply_once: true,
                counters_installed_before_entry_observers: true,
                checked_i32_storage_no_saturation: true,
            });
            ordered_rules.push(InstructionV66::ChosenTypeAnthem {
                effective_current_creatures_of_current_controller: true,
                current_fellowship_is_both_power_and_toughness_bonus: true,
                native_layer_7c_and_printed_ability_suppression: true,
                source_departure_ends_bonus: true,
            });
        }
        FamilyV66::SecludedCourtyard => {
            ordered_rules.push(InstructionV66::TapForUnrestrictedColorless {
                amount: 1,
                colors: vec![ManaColor::C],
                native_mana_ability_no_stack: true,
            });
            ordered_rules.push(InstructionV66::TapForRestrictedColor {
                amount: 1,
                alternatives: vec![
                    ManaColor::W,
                    ManaColor::U,
                    ManaColor::B,
                    ManaColor::R,
                    ManaColor::G,
                ],
                creature_spell_must_have_chosen_effective_type: true,
                creature_or_creature_card_activation_must_have_chosen_effective_type: true,
                includes_hand_and_graveyard_creature_card_abilities: true,
                changeling_matches_every_domain_type: true,
                cannot_pay_unrelated_spell_or_noncreature_ability: true,
                float_retains_producing_incarnation_and_choice_after_departure_control_or_reentry:
                    true,
                mana_clears_at_native_step_phase_boundary: true,
                native_mana_ability_no_stack: true,
            });
        }
    }
    Ok(MeaningV66 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        family,
        card_id: id,
        generic_mana: d.cost.generic,
        mana_value: d.mana_value,
        types: d.types.to_vec(),
        colors: d.colors.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: d
            .supertypes
            .iter()
            .map(|s| match s {
                Supertype::Basic => SupertypeV1::Basic,
                Supertype::Snow => SupertypeV1::Snow,
                Supertype::Legendary => SupertypeV1::Legendary,
            })
            .collect(),
        power: d.power,
        toughness: d.toughness,
        keyword_bits: d.keywords.0,
        is_land: d.is_land,
        ordered_rules,
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV66) -> Result<(), String> {
    if *value == printed_meaning(value.family)? {
        Ok(())
    } else {
        Err("V66 canonical ordered meaning changed".into())
    }
}
