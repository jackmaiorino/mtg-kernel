//! Closed printed Kaito program. Partial metadata grants no production admission.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/kaito-cunning-infiltrator/v75";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV75 {
    pub generic_mana: u8,
    pub colored_mana: Vec<crate::mana::ManaColor>,
    pub mana_value: u16,
    pub types: Vec<crate::card_def::CardType>,
    pub subtypes: Vec<crate::card_def::Subtype>,
    pub supertypes: Vec<crate::optional_trigger_public_v1::SupertypeV1>,
    pub colors: Vec<crate::mana::ManaColor>,
    pub starting_loyalty: u32,
    pub loyalty_costs: Vec<i32>,
    pub sorcery_speed_only: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NinjaPrintedV75 {
    pub card_id: u16,
    pub colors: Vec<crate::mana::ManaColor>,
    pub types: Vec<crate::card_def::CardType>,
    pub subtypes: Vec<crate::card_def::Subtype>,
    pub power: i32,
    pub toughness: i32,
    pub keyword_bits: u32,
    pub activated_abilities: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionV75 {
    CastPermanentForOneUUWithOrdinaryTimingAndResponses,
    LoyaltyAtSorceryTimingOncePerTurnOnThisPermanent,
    PayLoyaltyBeforePublishingAndKeepCostsWhenCountered,
    PlusOneCostIsNotAnEffectCounterPlacement,
    PlusOneTargetsZeroOrOneCreatureYouControl,
    RecheckOriginalTargetIncarnationAndControlOnResolution,
    SoleIllegalTargetFizzlesTheEntireAbility,
    SelectedOriginalCreatureCannotBeBlockedThisTurnAsARuleEffect,
    ThenDrawOneAndDiscardOneWithControllersPrivateChoice,
    EmptyLibraryDrawIsAttemptedAndOrdinaryLossRulesStillApply,
    MinusTwoCreatesOneTwoOneBlueNinjaWithNormalTokenReplacements,
    MinusNineCreatesASeparatePersistentEmblemForCapturedController,
    CaptureEveryFinalPositiveControlledCreatureCombatDamageToAPlayerBeforeTheNativeWave,
    PublishOneMandatoryLoyaltyTriggerPerObserverAndActualDamageOccurrence,
    SourceDepartureOrControlChangeDoesNotRewriteCapturedController,
    LoyaltyTriggerOnlyAddsToOriginalSurvivingIncarnationWithCurrentReplacements,
    EachEmblemObservesActualCastsByAnyPlayerIncludingLaterCounteredSpells,
    SpellCopiesAreNotCastsAndDoNotCreateEmblemCastOccurrences,
    EachEmblemTriggerCreatesItsOwnNinjaBatchWithNormalReplacements,
    DisposedIsDistinctFromResolvedCounteredAndFizzled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV75 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV75,
    pub ninja: NinjaPrintedV75,
    pub ordered_rules: Vec<InstructionV75>,
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
            include_str!("rules_meaning_kaito_v75.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn printed_meaning() -> Result<MeaningV75, String> {
    if !crate::effect::kaito_cunning_v75::definition(494)
        || !crate::effect::kaito_cunning_v75::token_definition(495)
    {
        return Err("V75 requires exact native Partial Kaito".into());
    }
    use InstructionV75::*;
    Ok(MeaningV75 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        card_id: 494,
        printed: PrintedV75 {
            generic_mana: 1,
            colored_mana: vec![crate::mana::ManaColor::U; 2],
            mana_value: 3,
            types: vec![crate::card_def::CardType::Planeswalker],
            subtypes: vec![crate::card_def::Subtype::Kaito],
            supertypes: vec![crate::optional_trigger_public_v1::SupertypeV1::Legendary],
            colors: vec![crate::mana::ManaColor::U],
            starting_loyalty: 3,
            loyalty_costs: vec![1, -2, -9],
            sorcery_speed_only: true,
        },
        ninja: NinjaPrintedV75 {
            card_id: 495,
            colors: vec![crate::mana::ManaColor::U],
            types: vec![crate::card_def::CardType::Creature],
            subtypes: vec![crate::card_def::Subtype::Ninja],
            power: 2,
            toughness: 1,
            keyword_bits: 0,
            activated_abilities: 0,
        },
        ordered_rules: vec![
            CastPermanentForOneUUWithOrdinaryTimingAndResponses,
            LoyaltyAtSorceryTimingOncePerTurnOnThisPermanent,
            PayLoyaltyBeforePublishingAndKeepCostsWhenCountered,
            PlusOneCostIsNotAnEffectCounterPlacement,
            PlusOneTargetsZeroOrOneCreatureYouControl,
            RecheckOriginalTargetIncarnationAndControlOnResolution,
            SoleIllegalTargetFizzlesTheEntireAbility,
            SelectedOriginalCreatureCannotBeBlockedThisTurnAsARuleEffect,
            ThenDrawOneAndDiscardOneWithControllersPrivateChoice,
            EmptyLibraryDrawIsAttemptedAndOrdinaryLossRulesStillApply,
            MinusTwoCreatesOneTwoOneBlueNinjaWithNormalTokenReplacements,
            MinusNineCreatesASeparatePersistentEmblemForCapturedController,
            CaptureEveryFinalPositiveControlledCreatureCombatDamageToAPlayerBeforeTheNativeWave,
            PublishOneMandatoryLoyaltyTriggerPerObserverAndActualDamageOccurrence,
            SourceDepartureOrControlChangeDoesNotRewriteCapturedController,
            LoyaltyTriggerOnlyAddsToOriginalSurvivingIncarnationWithCurrentReplacements,
            EachEmblemObservesActualCastsByAnyPlayerIncludingLaterCounteredSpells,
            SpellCopiesAreNotCastsAndDoNotCreateEmblemCastOccurrences,
            EachEmblemTriggerCreatesItsOwnNinjaBatchWithNormalReplacements,
            DisposedIsDistinctFromResolvedCounteredAndFizzled,
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(v: &MeaningV75) -> Result<(), String> {
    if *v != printed_meaning()? {
        return Err("V75 printed meaning changed".into());
    }
    Ok(())
}
