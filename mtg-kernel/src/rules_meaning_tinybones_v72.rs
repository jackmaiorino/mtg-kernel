//! Exact printed Tinybones program. Partial metadata grants no complete consumer.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/tinybones-bauble-burglar/v72";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV72 {
    pub generic_mana: u8,
    pub colored_mana: Vec<crate::mana::ManaColor>,
    pub mana_value: u16,
    pub colors: Vec<crate::mana::ManaColor>,
    pub activation_generic_mana: u8,
    pub activation_colored_mana: Vec<crate::mana::ManaColor>,
    pub activation_tap: bool,
    pub activation_sorcery_only: bool,
    pub types: Vec<crate::card_def::CardType>,
    pub subtypes: Vec<crate::card_def::Subtype>,
    pub supertypes: Vec<crate::optional_trigger_public_v1::SupertypeV1>,
    pub power: i32,
    pub toughness: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV72 {
    PayPrintedOneBlackAndEnterWithNormalCreatureTimingAndResponses,
    ObserveEveryActualOpponentDiscardBeforeItsReplacement,
    CaptureOriginalDiscardedCardAndEveryActiveFaceUpObserverController,
    PublishMandatoryUntargetedTriggerWithOriginalIncarnationsAndNativeApnap,
    OriginalGraveyardCardOnlyExilesWithOneStashCounter,
    MadnessAndOtherReplacementOrLeftReturnedCardDoNotBecomeOriginalGraveyardCard,
    SourceDepartureAndLaterControlDoNotChangeCapturedTrigger,
    CurrentActiveSourceGrantsOwnTurnPlayOfUnownedStashedExileCards,
    StashIssuerIsIndependentOfCurrentPermissionSource,
    PreserveNormalTimingLandQuotaOriginalCostsTargetsAdditionalCostsAndSpellForms,
    ActualCurrentSourceLetsManaMatchAnyTypeForForeignStashedCards,
    ManaMatchingIsDistinctFromOwnTurnPermissionAndPreservesConvokeAndPhyrexianLife,
    ActivateAtSorceryTimingByPayingThreeBlackAndTapWithOrdinarySickness,
    ActivationMakesEachOpponentDiscardOneCardWithTheirOwnChoice,
    SourceRemovalAndCounteringKeepAlreadyPaidCosts,
    NativeAnnouncementReversalPreservesOriginalStashAtReboundIncarnation,
    ForeignAdventureGrantsActualCasterItsPermanentFacePermission,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV72 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV72,
    pub ordered_rules: Vec<InstructionV72>,
    pub full_engine_admitted: bool,
    pub complete_consumer_admitted: bool,
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    static H: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    H.get_or_init(|| {
        let mut h = Sha256::new();
        for s in [
            SCHEMA,
            include_str!("rules_meaning_tinybones_v72.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn printed_meaning() -> Result<MeaningV72, String> {
    if !crate::engine::tinybones_v72::definition(490) {
        return Err("V72 requires exact native Partial Tinybones".into());
    }
    use InstructionV72::*;
    Ok(MeaningV72 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        card_id: 490,
        printed: PrintedV72 {
            generic_mana: 1,
            colored_mana: vec![crate::mana::ManaColor::B],
            mana_value: 2,
            colors: vec![crate::mana::ManaColor::B],
            activation_generic_mana: 3,
            activation_colored_mana: vec![crate::mana::ManaColor::B],
            activation_tap: true,
            activation_sorcery_only: true,
            types: vec![crate::card_def::CardType::Creature],
            subtypes: vec![
                crate::card_def::Subtype::Skeleton,
                crate::card_def::Subtype::Rogue,
            ],
            supertypes: vec![crate::optional_trigger_public_v1::SupertypeV1::Legendary],
            power: 1,
            toughness: 3,
        },
        ordered_rules: vec![
            PayPrintedOneBlackAndEnterWithNormalCreatureTimingAndResponses,
            ObserveEveryActualOpponentDiscardBeforeItsReplacement,
            CaptureOriginalDiscardedCardAndEveryActiveFaceUpObserverController,
            PublishMandatoryUntargetedTriggerWithOriginalIncarnationsAndNativeApnap,
            OriginalGraveyardCardOnlyExilesWithOneStashCounter,
            MadnessAndOtherReplacementOrLeftReturnedCardDoNotBecomeOriginalGraveyardCard,
            SourceDepartureAndLaterControlDoNotChangeCapturedTrigger,
            CurrentActiveSourceGrantsOwnTurnPlayOfUnownedStashedExileCards,
            StashIssuerIsIndependentOfCurrentPermissionSource,
            PreserveNormalTimingLandQuotaOriginalCostsTargetsAdditionalCostsAndSpellForms,
            ActualCurrentSourceLetsManaMatchAnyTypeForForeignStashedCards,
            ManaMatchingIsDistinctFromOwnTurnPermissionAndPreservesConvokeAndPhyrexianLife,
            ActivateAtSorceryTimingByPayingThreeBlackAndTapWithOrdinarySickness,
            ActivationMakesEachOpponentDiscardOneCardWithTheirOwnChoice,
            SourceRemovalAndCounteringKeepAlreadyPaidCosts,
            NativeAnnouncementReversalPreservesOriginalStashAtReboundIncarnation,
            ForeignAdventureGrantsActualCasterItsPermanentFacePermission,
        ],
        full_engine_admitted: false,
        complete_consumer_admitted: false,
    })
}
pub fn validate(v: &MeaningV72) -> Result<(), String> {
    if *v != printed_meaning()? {
        return Err("V72 printed program changed".into());
    }
    Ok(())
}
