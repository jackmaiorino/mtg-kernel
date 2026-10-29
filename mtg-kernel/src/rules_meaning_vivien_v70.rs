//! Closed printed Vivien program. Partial metadata grants no production admission.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/vivien-reid/v70";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV70 {
    pub generic_mana: u8,
    pub colored_mana: Vec<crate::mana::ManaColor>,
    pub mana_value: u16,
    pub types: Vec<crate::card_def::CardType>,
    pub subtypes: Vec<crate::card_def::Subtype>,
    pub supertypes: Vec<crate::optional_trigger_public_v1::SupertypeV1>,
    pub colors: Vec<crate::mana::ManaColor>,
    pub starting_loyalty: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionV70 {
    CastPermanentForThreeGGWithOrdinaryTimingAndResponses,
    LoyaltyAtSorceryTimingOncePerTurnOnThisPermanent,
    PayLoyaltyBeforePublishingAndKeepCostsWhenCountered,
    PlusOneIsACostUnaffectedByDoublingSeason,
    LookAtUpToFourTopCardsPrivately,
    MayRevealOneCreatureOrLandCardAndPutItIntoYourHand,
    PutAllRemainingLookedAtCardsOnLibraryBottomInRandomOrder,
    MinusThreeTargetsArtifactOrEnchantmentOrCreatureWithFlying,
    RecheckTheOriginalTargetIncarnationAndLegalityOnResolution,
    DestroyThatTargetWithNormalIndestructibleAndReplacementRules,
    MinusEightCreatesAnEmblemForCapturedController,
    EmblemPersistsAfterSourceLeavesOrChangesController,
    CurrentCreaturesYouControlGetPlusTwoPlusTwo,
    CurrentCreaturesYouControlHaveVigilanceTrampleIndestructible,
    ApplyTimestampedLayerSixAbilityGrantsAndLayerSevenCounterAndModifierRules,
    DisposedIsDistinctFromResolvedOrCountered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV70 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV70,
    pub ordered_rules: Vec<InstructionV70>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    use sha2::{Digest, Sha256};
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for s in [
            SCHEMA,
            include_str!("rules_meaning_vivien_v70.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn printed_meaning() -> Result<MeaningV70, String> {
    if !crate::effect::vivien_reid_v1::definition(488) {
        return Err("V70 requires exact native Partial Vivien".into());
    }
    use InstructionV70::*;
    Ok(MeaningV70 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        card_id: 488,
        printed: PrintedV70 {
            generic_mana: 3,
            colored_mana: vec![crate::mana::ManaColor::G; 2],
            mana_value: 5,
            types: vec![crate::card_def::CardType::Planeswalker],
            subtypes: vec![crate::card_def::Subtype::Vivien],
            supertypes: vec![crate::optional_trigger_public_v1::SupertypeV1::Legendary],
            colors: vec![crate::mana::ManaColor::G],
            starting_loyalty: 5,
        },
        ordered_rules: vec![
            CastPermanentForThreeGGWithOrdinaryTimingAndResponses,
            LoyaltyAtSorceryTimingOncePerTurnOnThisPermanent,
            PayLoyaltyBeforePublishingAndKeepCostsWhenCountered,
            PlusOneIsACostUnaffectedByDoublingSeason,
            LookAtUpToFourTopCardsPrivately,
            MayRevealOneCreatureOrLandCardAndPutItIntoYourHand,
            PutAllRemainingLookedAtCardsOnLibraryBottomInRandomOrder,
            MinusThreeTargetsArtifactOrEnchantmentOrCreatureWithFlying,
            RecheckTheOriginalTargetIncarnationAndLegalityOnResolution,
            DestroyThatTargetWithNormalIndestructibleAndReplacementRules,
            MinusEightCreatesAnEmblemForCapturedController,
            EmblemPersistsAfterSourceLeavesOrChangesController,
            CurrentCreaturesYouControlGetPlusTwoPlusTwo,
            CurrentCreaturesYouControlHaveVigilanceTrampleIndestructible,
            ApplyTimestampedLayerSixAbilityGrantsAndLayerSevenCounterAndModifierRules,
            DisposedIsDistinctFromResolvedOrCountered,
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(v: &MeaningV70) -> Result<(), String> {
    if *v != printed_meaning()? {
        return Err("V70 printed meaning changed".into());
    }
    Ok(())
}
