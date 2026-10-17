//! Exact native Partial493 program. This appendix grants no complete consumer.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/chandra-flameshaper/v74";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV74 {
    pub generic_mana: u8,
    pub colored_mana: Vec<crate::mana::ManaColor>,
    pub mana_value: u16,
    pub colors: Vec<crate::mana::ManaColor>,
    pub types: Vec<crate::card_def::CardType>,
    pub subtypes: Vec<crate::card_def::Subtype>,
    pub supertypes: Vec<crate::optional_trigger_public_v1::SupertypeV1>,
    pub starting_loyalty: u32,
    pub loyalty_costs: [i32; 3],
    pub loyalty_sorcery_only: bool,
    pub plus_two_red_mana: u8,
    pub plus_two_exile_count: u8,
    pub plus_two_choose_count: u8,
    pub minus_four_damage: u8,
    pub minus_four_maximum_targets: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionV74 {
    PayFiveRedRedWithOrdinaryPlaneswalkerTimingAndResponses,
    EnterWithSixLoyaltySubjectToActualCounterReplacements,
    ActivateAtSorceryTimingWithOrdinaryOncePerTurnLoyaltyRestriction,
    PayOriginalPlusTwoPlusOneOrMinusFourLoyaltyAsCostWithoutDoublingSeason,
    PaidAbilityKeepsOriginalSourceControllerAfterSourceDepartureOrControlChange,
    PlusTwoAddsThreeRedManaAndPubliclyExilesTopThreeCardsOrAllIfFewer,
    ChooseExactlyOneOfThosePublicExiledCardsWhenAnyExist,
    ChosenOriginalExileCardMayBePlayedThisTurnWithOrdinaryCostsTimingAndLandQuota,
    GrantNeverExtendsToOtherExiledCardsOrNewIncarnationsAndExpiresAtCleanup,
    PlusOneTargetsOneCreatureYouControlAndCopiesItsCopiableValues,
    CopyHasCopiableHasteAndOneAdditionalCopiableBeginningOfEndStepSacrificeAbility,
    CopyOfCopyRetainsExistingSacrificeAbilityMultiplicity,
    TokenCreationUsesActualReplacementsIncludingDoublingSeason,
    MinusFourChoosesZeroThroughEightDistinctCreatureOrPlaneswalkerTargetsBeforePayment,
    EachChosenTargetReceivesAtLeastOneOfEightOneUnitAllocationsBeforePayment,
    OriginalTargetsAndDivisionSurviveIndividualTargetLossWithoutRedistribution,
    ZeroTargetsIsLegalAndDealsNoDamage,
    SourceDepartureKeepsOriginalDamageIncarnationAndNativeLastKnownDamageFacts,
    ActualAffectedPlayersChooseCrystalReplacementOrderingForOriginalSimultaneousBatch,
    CounteredOrDisposedAbilityKeepsPaidLoyaltyWithoutInventedResolution,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV74 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV74,
    pub ordered_rules: Vec<InstructionV74>,
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
            include_str!("rules_meaning_chandra_v74.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn printed_meaning() -> Result<MeaningV74, String> {
    let native = crate::effect::chandra_flameshaper_v1::definition(493);
    if !native {
        return Err("V74 requires exact native Partial Chandra".into());
    }
    use crate::{
        card_def::{CardType, Subtype},
        mana::ManaColor::R,
    };
    use InstructionV74::*;
    Ok(MeaningV74 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        card_id: 493,
        printed: PrintedV74 {
            generic_mana: 5,
            colored_mana: vec![R, R],
            mana_value: 7,
            colors: vec![R],
            types: vec![CardType::Planeswalker],
            subtypes: vec![Subtype::Chandra],
            supertypes: vec![crate::optional_trigger_public_v1::SupertypeV1::Legendary],
            starting_loyalty: 6,
            loyalty_costs: [2, 1, -4],
            loyalty_sorcery_only: true,
            plus_two_red_mana: 3,
            plus_two_exile_count: 3,
            plus_two_choose_count: 1,
            minus_four_damage: 8,
            minus_four_maximum_targets: 8,
        },
        ordered_rules: vec![
            PayFiveRedRedWithOrdinaryPlaneswalkerTimingAndResponses,
            EnterWithSixLoyaltySubjectToActualCounterReplacements,
            ActivateAtSorceryTimingWithOrdinaryOncePerTurnLoyaltyRestriction,
            PayOriginalPlusTwoPlusOneOrMinusFourLoyaltyAsCostWithoutDoublingSeason,
            PaidAbilityKeepsOriginalSourceControllerAfterSourceDepartureOrControlChange,
            PlusTwoAddsThreeRedManaAndPubliclyExilesTopThreeCardsOrAllIfFewer,
            ChooseExactlyOneOfThosePublicExiledCardsWhenAnyExist,
            ChosenOriginalExileCardMayBePlayedThisTurnWithOrdinaryCostsTimingAndLandQuota,
            GrantNeverExtendsToOtherExiledCardsOrNewIncarnationsAndExpiresAtCleanup,
            PlusOneTargetsOneCreatureYouControlAndCopiesItsCopiableValues,
            CopyHasCopiableHasteAndOneAdditionalCopiableBeginningOfEndStepSacrificeAbility,
            CopyOfCopyRetainsExistingSacrificeAbilityMultiplicity,
            TokenCreationUsesActualReplacementsIncludingDoublingSeason,
            MinusFourChoosesZeroThroughEightDistinctCreatureOrPlaneswalkerTargetsBeforePayment,
            EachChosenTargetReceivesAtLeastOneOfEightOneUnitAllocationsBeforePayment,
            OriginalTargetsAndDivisionSurviveIndividualTargetLossWithoutRedistribution,
            ZeroTargetsIsLegalAndDealsNoDamage,
            SourceDepartureKeepsOriginalDamageIncarnationAndNativeLastKnownDamageFacts,
            ActualAffectedPlayersChooseCrystalReplacementOrderingForOriginalSimultaneousBatch,
            CounteredOrDisposedAbilityKeepsPaidLoyaltyWithoutInventedResolution,
        ],
        full_engine_admitted: false,
        complete_consumer_admitted: false,
    })
}
pub fn validate(v: &MeaningV74) -> Result<(), String> {
    if *v != printed_meaning()? {
        return Err("V74 printed program changed".into());
    }
    Ok(())
}
