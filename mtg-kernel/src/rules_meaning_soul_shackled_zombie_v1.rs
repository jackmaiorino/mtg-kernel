//! Exact definition-owned companion; frozen RulesVector/model inputs retain their refusal.
use crate::card_def::{self, TargetSpec};
use crate::effect::EffectOp;
use crate::state::Zone;
use crate::trigger::{self, TriggerCondition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/soul-shackled-zombie/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingleGraveyardTargetV1 {
    pub minimum: u8,
    pub maximum: u8,
    pub physical_owner_defines_graveyard: bool,
    pub either_players_graveyard: bool,
    pub distinct_cards: bool,
    pub excludes_tokens_and_spell_copies: bool,
    pub exact_selected_incarnations: bool,
    pub individual_legality_rechecked_before_moves: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuccessfulExileDrainV1 {
    pub creature_type_remembered_before_move: bool,
    pub requires_actual_successful_creature_exile: bool,
    pub uses_replacement_pipeline: bool,
    pub drain_once_if_one_or_two_creatures_exiled: bool,
    pub opponent_loss: u8,
    pub controller_gain: u8,
    pub opponent_untargeted: bool,
    pub gain_recipient_original_trigger_controller: bool,
    pub source_departure_or_control_change_does_not_change_recipient: bool,
    pub legal_partial_targets_resolve: bool,
    pub zero_targets_noop: bool,
    pub all_announced_targets_illegal_fizzles: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZombieRootMeaningV1 {
    pub schema: String,
    pub catalog_hash: u64,
    pub source_sha256: String,
    pub card_id: u16,
    pub trigger_index: u32,
    pub ordinary_battlefield_etb: bool,
    pub independent_committed_entry_controller: bool,
    pub target: SingleGraveyardTargetV1,
    pub effect: SuccessfulExileDrainV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningErrorV1 {
    UnsupportedRoot,
    IdentityMismatch,
}
pub fn source_sha256() -> String {
    let mut digest = Sha256::new();
    for source in [
        include_str!("rules_meaning_soul_shackled_zombie_v1.rs"),
        include_str!("soul_shackled_zombie_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("card_def.rs"),
        include_str!("state.rs"),
        include_str!("../build.rs"),
    ] {
        digest.update([0]);
        digest.update(source.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
/// Selects only an actual executable registered definition and its exact ETB.
pub fn root_meaning(
    card_id: u16,
    trigger_index: u32,
) -> Result<ZombieRootMeaningV1, MeaningErrorV1> {
    let def = card_def::CARD_DEFS
        .get(card_id as usize)
        .ok_or(MeaningErrorV1::UnsupportedRoot)?;
    let ability = trigger::triggers_for(card_id)
        .get(trigger_index as usize)
        .ok_or(MeaningErrorV1::UnsupportedRoot)?;
    let effect = (ability.effect)();
    if !def.is_executable()
        || def.name != "Soul-Shackled Zombie"
        || ability.condition != TriggerCondition::Etb
        || ability.home_zone != Zone::Battlefield
        || ability.face_index != 0
        || ability.intervening_if_kicked
        || ability.intervening_if_controls_another_source_card
        || effect != EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1
        || trigger::target_spec_for_trigger(card_id, &effect)
            != Some(TargetSpec::UpToTwoCardsInSingleGraveyard)
    {
        return Err(MeaningErrorV1::UnsupportedRoot);
    }
    Ok(ZombieRootMeaningV1 {
        schema: SCHEMA.into(),
        catalog_hash: card_def::KERNEL_CARDDB_HASH,
        source_sha256: source_sha256(),
        card_id,
        trigger_index,
        ordinary_battlefield_etb: true,
        independent_committed_entry_controller: true,
        target: SingleGraveyardTargetV1 {
            minimum: 0,
            maximum: 2,
            physical_owner_defines_graveyard: true,
            either_players_graveyard: true,
            distinct_cards: true,
            excludes_tokens_and_spell_copies: true,
            exact_selected_incarnations: true,
            individual_legality_rechecked_before_moves: true,
        },
        effect: SuccessfulExileDrainV1 {
            creature_type_remembered_before_move: true,
            requires_actual_successful_creature_exile: true,
            uses_replacement_pipeline: true,
            drain_once_if_one_or_two_creatures_exiled: true,
            opponent_loss: 2,
            controller_gain: 2,
            opponent_untargeted: true,
            gain_recipient_original_trigger_controller: true,
            source_departure_or_control_change_does_not_change_recipient: true,
            legal_partial_targets_resolve: true,
            zero_targets_noop: true,
            all_announced_targets_illegal_fizzles: true,
        },
    })
}
pub fn validate(record: &ZombieRootMeaningV1) -> Result<(), MeaningErrorV1> {
    if *record != root_meaning(record.card_id, record.trigger_index)? {
        return Err(MeaningErrorV1::IdentityMismatch);
    }
    Ok(())
}
pub fn catalog_roots() -> Vec<ZombieRootMeaningV1> {
    card_def::CARD_DEFS
        .iter()
        .enumerate()
        .flat_map(|(id, _)| {
            trigger::triggers_for(id as u16)
                .iter()
                .enumerate()
                .filter_map(move |(index, _)| root_meaning(id as u16, index as u32).ok())
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_registered_definitions_cannot_supply_exact_zombie_meaning() {
        let id = card_def::card_id_by_name("Llanowar Elves").unwrap();
        assert_eq!(root_meaning(id, 0), Err(MeaningErrorV1::UnsupportedRoot));
        assert_eq!(
            root_meaning(u16::MAX, 0),
            Err(MeaningErrorV1::UnsupportedRoot)
        );
        assert_eq!(source_sha256().len(), 64);
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn future_registered_root_binds_exact_optional_targets_and_successful_exile() {
        let id =
            card_def::card_id_by_name("Soul-Shackled Zombie").expect("future catalog prerequisite");
        let record = root_meaning(id, 0).unwrap();
        validate(
            &serde_json::from_slice::<ZombieRootMeaningV1>(&serde_json::to_vec(&record).unwrap())
                .unwrap(),
        )
        .unwrap();
        for field in 0..8 {
            let mut forged = record.clone();
            match field {
                0 => forged.schema.push('x'),
                1 => forged.catalog_hash ^= 1,
                2 => forged.source_sha256.push('x'),
                3 => forged.target.minimum = 1,
                4 => forged.target.physical_owner_defines_graveyard = false,
                5 => forged.effect.requires_actual_successful_creature_exile = false,
                6 => forged.effect.opponent_untargeted = false,
                _ => forged.trigger_index = 1,
            }
            assert!(validate(&forged).is_err());
        }
    }
}
