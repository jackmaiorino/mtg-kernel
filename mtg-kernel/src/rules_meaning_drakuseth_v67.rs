//! Closed FDN193 printed program, without executable consumer admission.
use crate::{
    card_def::{
        self, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
        CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    optional_trigger_public_v1::SupertypeV1,
    rules_meaning_graveyard_return_v61::ordinary_foreign_fields_absent,
    standard_cards_v1::{self, StandardOpV1, StandardTargetV1},
    state::Zone,
    trigger::TriggerCondition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "rules-meaning/drakuseth/v67";
pub const NAME: &str = "Drakuseth, Maw of Flames";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV67 {
    pub generic_mana: u8,
    pub colored_mana: Vec<ManaColor>,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<SupertypeV1>,
    pub colors: Vec<ManaColor>,
    pub power: i16,
    pub toughness: i16,
    pub keyword_bits: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV67 {
    PayManaAndCast {
        generic: u8,
        red: u8,
        ordinary_native_permission_and_priority: bool,
        spell_targets: u8,
        normal_entry_and_countering_replacements: bool,
        enters_under_original_spell_controller: bool,
        countered_spell_uses_physical_owner_graveyard: bool,
    },
    Flying,
    OnThisCreatureAttacking {
        actual_declaration_not_becoming_attacking_or_damage: bool,
        source_need_not_survive_or_retain_controller: bool,
        captured_original_source_and_controller: bool,
        mandatory_any_targets: u8,
        optional_other_any_targets: u8,
        all_original_targets_distinct: bool,
        choose_on_stack_placement_before_responses: bool,
        allow_zero_optional_targets: bool,
    },
    DamageOriginalTargetSlots {
        first_amount: i32,
        each_other_amount: i32,
        amounts_follow_original_slots_after_individual_illegality: bool,
        original_target_incarnations_and_current_legality: bool,
        all_targets_illegal_counters_ability: bool,
        one_simultaneous_damage_batch_before_state_based_actions: bool,
        prevention_and_damage_replacements_apply: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV67 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV67,
    pub ordered_rules: Vec<RuleV67>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let predecessor = crate::rules_meaning_embercleave_v65::producer_sha256();
        let mut hash = Sha256::new();
        for text in [
            SCHEMA,
            predecessor.as_str(),
            include_str!("rules_meaning_drakuseth_v67.rs"),
            include_str!("drakuseth_public_v67.rs"),
            include_str!("drakuseth_v1_tests.rs"),
            include_str!("lib.rs"),
        ] {
            hash.update((text.len() as u64).to_le_bytes());
            hash.update(text.as_bytes());
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub(crate) fn target_spec() -> TargetSpec {
    TargetSpec::StandardV1(StandardTargetV1::OneToThreeDistinctAnyTargetsV1)
}
pub(crate) fn selected_id() -> Result<u16, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("V67 requires the actual pure Limited printing".into());
    }
    let id = card_def::card_id_by_name(NAME).ok_or("V67 printing absent")?;
    exact_definition(id, &CARD_DEFS[usize::from(id)])?;
    Ok(id)
}
fn exact_definition(id: u16, d: &CardDef) -> Result<(), String> {
    if d.capability != CardCapability::Partial {
        return Err("V67 requires its original Partial catalog boundary".into());
    }
    exact_native_definition_v76(id, d)
}
/// Read-only source qualification may precede catalog promotion. This exact
/// shape check grants neither a policy owner nor a full-deck admission.
pub(crate) fn exact_native_definition_v76(id: u16, d: &CardDef) -> Result<(), String> {
    // The predecessor helper exhaustively destructures every CardDef field.
    if id != 464
        || !ordinary_foreign_fields_absent(d)
        || d.name != NAME
        || !matches!(d.capability, CardCapability::Partial | CardCapability::Full)
        || d.cost.generic != 4
        || d.cost.x_count != 0
        || d.cost.pips != [Pip::Colored(ManaColor::R); 3]
        || d.mana_value != 7
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Dragon]
        || d.supertypes != [Supertype::Legendary]
        || d.colors != [ManaColor::R]
        || d.power != Some(7)
        || d.toughness != Some(7)
        || d.keywords != Keywords::FLYING
        || d.target_spec != TargetSpec::None
        || d.is_token
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err("V67 complete Drakuseth definition changed".into());
    }
    let [t] = crate::trigger::triggers_for(id) else {
        return Err("V67 attack program roster changed".into());
    };
    if t.condition != TriggerCondition::Attacks
        || t.home_zone != Zone::Battlefield
        || t.intervening_if_kicked
        || t.intervening_if_controls_another_source_card
        || t.face_index != 0
        || (t.effect)()
            != EffectOp::StandardV1(StandardOpV1::DamageTargetsByPositionV1 {
                first_amount: 4,
                other_amount: 3,
            })
        || (t.effect)() != standard_cards_v1::drakuseth_attack_v1()
        || crate::trigger::target_spec_for_trigger(id, &(t.effect)()) != Some(target_spec())
    {
        return Err("V67 complete native attack/target program changed".into());
    }
    Ok(())
}
pub fn printed_meaning() -> Result<MeaningV67, String> {
    let id = selected_id()?;
    let d = &CARD_DEFS[usize::from(id)];
    Ok(MeaningV67 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: id,
        printed: PrintedV67 {
            generic_mana: 4,
            colored_mana: vec![ManaColor::R; 3],
            mana_value: 7,
            types: d.types.to_vec(),
            subtypes: d.subtypes.to_vec(),
            supertypes: vec![SupertypeV1::Legendary],
            colors: d.colors.to_vec(),
            power: 7,
            toughness: 7,
            keyword_bits: Keywords::FLYING.0,
        },
        ordered_rules: vec![
            RuleV67::PayManaAndCast {
                generic: 4,
                red: 3,
                ordinary_native_permission_and_priority: true,
                spell_targets: 0,
                normal_entry_and_countering_replacements: true,
                enters_under_original_spell_controller: true,
                countered_spell_uses_physical_owner_graveyard: true,
            },
            RuleV67::Flying,
            RuleV67::OnThisCreatureAttacking {
                actual_declaration_not_becoming_attacking_or_damage: true,
                source_need_not_survive_or_retain_controller: true,
                captured_original_source_and_controller: true,
                mandatory_any_targets: 1,
                optional_other_any_targets: 2,
                all_original_targets_distinct: true,
                choose_on_stack_placement_before_responses: true,
                allow_zero_optional_targets: true,
            },
            RuleV67::DamageOriginalTargetSlots {
                first_amount: 4,
                each_other_amount: 3,
                amounts_follow_original_slots_after_individual_illegality: true,
                original_target_incarnations_and_current_legality: true,
                all_targets_illegal_counters_ability: true,
                one_simultaneous_damage_batch_before_state_based_actions: true,
                prevention_and_damage_replacements_apply: true,
            },
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV67) -> Result<(), String> {
    if *value == printed_meaning()? {
        Ok(())
    } else {
        Err("V67 complete meaning changed".into())
    }
}
