//! Selected printed spell meaning. No observation, tensor or model admission.
use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef, PlayerRef};
use crate::mana::{Cost, ManaColor, Pip};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/involuntary-employment/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RootV1 {
    PrintedSpell,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleV1 {
    pub mandatory_one_creature_target_any_controller: bool,
    pub original_incarnation_and_current_target_legality_rechecked: bool,
    pub only_target_illegal_fizzles_including_treasure: bool,
    pub gain_control_uses_captured_spell_controller: bool,
    pub owner_zone_incarnation_and_attachments_retained: bool,
    pub actual_control_change_removes_from_combat_and_sets_sickness: bool,
    pub already_controlled_target_still_has_an_expiring_control_record: bool,
    pub untap_then_haste_then_treasure_in_printed_order: bool,
    pub stun_replaces_untap_without_preventing_haste_or_treasure: bool,
    pub treasure_is_created_under_captured_spell_controller: bool,
    pub source_departure_does_not_end_resolved_effect: bool,
    pub recipient_ability_loss_does_not_end_control_effect: bool,
    pub later_ability_removal_can_suppress_granted_haste: bool,
    pub overlapping_same_duration_records_unwind_newest_first: bool,
    pub cleanup_restores_previous_controller_not_physical_owner: bool,
    pub departure_reentry_does_not_inherit_control_or_haste: bool,
    pub cleanup_priority_installation_expires_before_player_flip: bool,
    pub cleanup_does_not_untap_or_remove_treasure: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
    pub public_human_sampler_and_model_admitted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    IdentityMismatch,
}
pub fn producer_sha256() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_employment_v1.rs"),
        include_str!("effect/temporary_control_v1.rs"),
        include_str!("effect.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector() -> Result<SelectorV1, ErrorV1> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV1::UnsupportedCatalog);
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(selector: &SelectorV1, card_id: u16, root: RootV1) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Involuntary Employment" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let token_def = crate::card_def::card_id_by_name("Treasure Token").ok_or(ErrorV1::WrongRoot)?;
    let expected = EffectOp::Sequence(vec![
        EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
        EffectOp::UntapObject {
            object: ObjectRef::Target(0),
        },
        EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::Target(0),
            keyword: Keywords::HASTE,
        },
        EffectOp::CreateToken {
            token_def,
            controller: PlayerRef::Controller,
        },
    ]);
    if def.cost
        != (Cost {
            generic: 3,
            pips: &[Pip::Colored(ManaColor::R)],
            x_count: 0,
        })
        || def.mana_value != 4
        || def.types != [CardType::Sorcery]
        || def.colors != [ManaColor::R]
        || !def.subtypes.is_empty()
        || def.power.is_some()
        || def.toughness.is_some()
        || def.keywords != Keywords::NONE
        || def.target_spec != TargetSpec::Creature
        || !def.activated_abilities.is_empty()
        || def.mode2.is_some()
        || def.mode3.is_some()
        || (def.spell_effect)() != Some(expected.clone())
    {
        return Err(ErrorV1::WrongRoot);
    }
    let rule = RuleV1 {
        mandatory_one_creature_target_any_controller: true,
        original_incarnation_and_current_target_legality_rechecked: true,
        only_target_illegal_fizzles_including_treasure: true,
        gain_control_uses_captured_spell_controller: true,
        owner_zone_incarnation_and_attachments_retained: true,
        actual_control_change_removes_from_combat_and_sets_sickness: true,
        already_controlled_target_still_has_an_expiring_control_record: true,
        untap_then_haste_then_treasure_in_printed_order: true,
        stun_replaces_untap_without_preventing_haste_or_treasure: true,
        treasure_is_created_under_captured_spell_controller: true,
        source_departure_does_not_end_resolved_effect: true,
        recipient_ability_loss_does_not_end_control_effect: true,
        later_ability_removal_can_suppress_granted_haste: true,
        overlapping_same_duration_records_unwind_newest_first: true,
        cleanup_restores_previous_controller_not_physical_owner: true,
        departure_reentry_does_not_inherit_control_or_haste: true,
        cleanup_priority_installation_expires_before_player_flip: true,
        cleanup_does_not_untap_or_remove_treasure: true,
    };
    let bytes = serde_json::to_vec(&(card_id, root, def.cost, def.target_spec, &expected, &rule))
        .expect("printed root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        root,
        root_sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        rule,
        public_human_sampler_and_model_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id, record.root)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_identity_refuses_wrong_schema_catalog_source_before_lookup() {
        let Ok(selector) = live_selector() else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        for field in 0..3 {
            let mut wrong = selector.clone();
            match field {
                0 => wrong.schema.push('x'),
                1 => wrong.card_db_hash ^= 1,
                _ => wrong.producer_sha256.push('x'),
            }
            assert_eq!(
                meaning(&wrong, u16::MAX, RootV1::PrintedSpell),
                Err(ErrorV1::WrongSelector)
            );
        }
        assert_eq!(
            meaning(
                &selector,
                crate::card_def::card_id_by_name("Lightning Bolt").unwrap(),
                RootV1::PrintedSpell
            ),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
}
