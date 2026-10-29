//! Exact closed printed Time Stop program. Partial metadata grants no admission.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/time-stop/v68";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV68 {
    pub generic_mana: u8,
    pub colored_mana: Vec<crate::mana::ManaColor>,
    pub mana_value: u16,
    pub instant: bool,
    pub targets: u8,
    pub x_count: u8,
    pub types: Vec<crate::card_def::CardType>,
    pub subtypes: Vec<crate::card_def::Subtype>,
    pub colors: Vec<crate::mana::ManaColor>,
    pub legendary: bool,
    pub power: Option<i16>,
    pub toughness: Option<i16>,
    pub keyword_bits: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionV68 {
    PayPrintedManaAndAllowResponses,
    InstantTimingRequiresNativeCastPermissionAndPriority,
    CounteredSpellDoesNotEndTheTurn,
    GenuineVirtualSpellCopiesUseTheSameEndTurnProgram,
    CeaseEarlierUnpublishedTriggers,
    ExileEveryOriginalStackObjectIncludingResolvingSpell,
    CheckStateBasedActionsWithoutPriorityOrTriggerPlacement,
    EndCurrentStepRemoveCombatAndSkipToCleanup,
    DiscardToCurrentMaximumHandSizeBeforeExpiration,
    RemoveDamageAndExpireCleanupDurations,
    CheckStateBasedActionsThenPlaceNewTriggersAndGrantCleanupPriority,
    RepeatEntireCleanupAfterCompletedPriorityWindow,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV68 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV68,
    pub ordered_rules: Vec<InstructionV68>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn printed_meaning() -> Result<MeaningV68, String> {
    if !crate::engine::time_stop_v1::definition(486) {
        return Err("V68 requires the exact native Partial Time Stop definition".into());
    }
    use InstructionV68::*;
    Ok(MeaningV68 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        card_id: 486,
        printed: PrintedV68 {
            generic_mana: 4,
            colored_mana: vec![crate::mana::ManaColor::U; 2],
            mana_value: 6,
            instant: true,
            targets: 0,
            x_count: 0,
            types: vec![crate::card_def::CardType::Instant],
            subtypes: vec![],
            colors: vec![crate::mana::ManaColor::U],
            legendary: false,
            power: None,
            toughness: None,
            keyword_bits: 0,
        },
        ordered_rules: vec![
            PayPrintedManaAndAllowResponses,
            InstantTimingRequiresNativeCastPermissionAndPriority,
            CounteredSpellDoesNotEndTheTurn,
            GenuineVirtualSpellCopiesUseTheSameEndTurnProgram,
            CeaseEarlierUnpublishedTriggers,
            ExileEveryOriginalStackObjectIncludingResolvingSpell,
            CheckStateBasedActionsWithoutPriorityOrTriggerPlacement,
            EndCurrentStepRemoveCombatAndSkipToCleanup,
            DiscardToCurrentMaximumHandSizeBeforeExpiration,
            RemoveDamageAndExpireCleanupDurations,
            CheckStateBasedActionsThenPlaceNewTriggersAndGrantCleanupPriority,
            RepeatEntireCleanupAfterCompletedPriorityWindow,
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_time_stop_v68.rs"),
        crate::engine::time_stop_v1::source_sha256_v1(),
    ] {
        h.update((source.len() as u64).to_le_bytes());
        h.update(source.as_bytes());
    }
    format!("{:x}", h.finalize())
}
pub fn validate(value: &MeaningV68) -> Result<(), String> {
    if *value != printed_meaning()? {
        return Err("V68 printed ordered meaning changed".into());
    }
    Ok(())
}
