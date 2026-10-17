//! Exact native Partial491 program. This appendix grants no complete consumer.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/liliana-dreadhorde-general/v73";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV73 {
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
    pub zombie_card_id: u16,
    pub zombie_power: i32,
    pub zombie_toughness: i32,
    pub zombie_colors: Vec<crate::mana::ManaColor>,
    pub zombie_types: Vec<crate::card_def::CardType>,
    pub zombie_subtypes: Vec<crate::card_def::Subtype>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionV73 {
    PayFourBlackBlackWithOrdinaryPlaneswalkerTimingAndResponses,
    EnterWithSixLoyaltySubjectToActualCounterReplacements,
    CaptureEveryControlledCreatureDeathIncludingTokensAndSelfIfCreatureBeforeTheBatch,
    CaptureOriginalActiveObserverControllerAndIncarnationUsingLastKnownInformation,
    MandatoryUntargetedDrawOneTriggerUsesNativeApnapAndSurvivesSourceDeparture,
    ActivateOnePrintedLoyaltyAbilityAtSorceryTimingWithOrdinaryOncePerTurnRestriction,
    PayOriginalPlusOneMinusFourOrMinusNineLoyaltyAsCostWithoutDoublingSeason,
    PaidAbilityKeepsOriginalSourceControllerAfterSourceDepartureOrControlChange,
    PlusOneCreatesOneBlackTwoTwoZombieSubjectToActualTokenReplacements,
    MinusFourEachPlayerChoosesTwoCreaturesOrAllIfFewerInApnapOrder,
    AllChoicesAndPhysicalOwnerGraveyardOrdersPrecedeOneSimultaneousSacrificeBatch,
    MinusNineEachOpponentChoosesOnePermanentForEachRepresentedPermanentType,
    SameMultitypePermanentMayBeChosenForSeveralTypesAndKeeperUnionSurvives,
    SacrificeAllOtherOpponentPermanentsIgnoringHexproofAndIndestructibility,
    SourceLostToZeroLoyaltyBeforeResolutionCannotObserveLaterCreatureDeaths,
    CounteredOrDisposedAbilityKeepsPaidLoyaltyWithoutInventedResolution,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV73 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV73,
    pub ordered_rules: Vec<InstructionV73>,
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
            include_str!("rules_meaning_liliana_v73.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn printed_meaning() -> Result<MeaningV73, String> {
    let native = crate::effect::liliana_dreadhorde_v1::definition(491)
        && crate::effect::liliana_dreadhorde_v1::token_definition(492);
    if !native {
        return Err("V73 requires exact native Partial Liliana and Zombie".into());
    }
    use crate::{
        card_def::{CardType, Subtype},
        mana::ManaColor::B,
    };
    use InstructionV73::*;
    Ok(MeaningV73 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        card_id: 491,
        printed: PrintedV73 {
            generic_mana: 4,
            colored_mana: vec![B, B],
            mana_value: 6,
            colors: vec![B],
            types: vec![CardType::Planeswalker],
            subtypes: vec![Subtype::Liliana],
            supertypes: vec![crate::optional_trigger_public_v1::SupertypeV1::Legendary],
            starting_loyalty: 6,
            loyalty_costs: [1, -4, -9],
            loyalty_sorcery_only: true,
            zombie_card_id: 492,
            zombie_power: 2,
            zombie_toughness: 2,
            zombie_colors: vec![B],
            zombie_types: vec![CardType::Creature],
            zombie_subtypes: vec![Subtype::Zombie],
        },
        ordered_rules: vec![
            PayFourBlackBlackWithOrdinaryPlaneswalkerTimingAndResponses,
            EnterWithSixLoyaltySubjectToActualCounterReplacements,
            CaptureEveryControlledCreatureDeathIncludingTokensAndSelfIfCreatureBeforeTheBatch,
            CaptureOriginalActiveObserverControllerAndIncarnationUsingLastKnownInformation,
            MandatoryUntargetedDrawOneTriggerUsesNativeApnapAndSurvivesSourceDeparture,
            ActivateOnePrintedLoyaltyAbilityAtSorceryTimingWithOrdinaryOncePerTurnRestriction,
            PayOriginalPlusOneMinusFourOrMinusNineLoyaltyAsCostWithoutDoublingSeason,
            PaidAbilityKeepsOriginalSourceControllerAfterSourceDepartureOrControlChange,
            PlusOneCreatesOneBlackTwoTwoZombieSubjectToActualTokenReplacements,
            MinusFourEachPlayerChoosesTwoCreaturesOrAllIfFewerInApnapOrder,
            AllChoicesAndPhysicalOwnerGraveyardOrdersPrecedeOneSimultaneousSacrificeBatch,
            MinusNineEachOpponentChoosesOnePermanentForEachRepresentedPermanentType,
            SameMultitypePermanentMayBeChosenForSeveralTypesAndKeeperUnionSurvives,
            SacrificeAllOtherOpponentPermanentsIgnoringHexproofAndIndestructibility,
            SourceLostToZeroLoyaltyBeforeResolutionCannotObserveLaterCreatureDeaths,
            CounteredOrDisposedAbilityKeepsPaidLoyaltyWithoutInventedResolution,
        ],
        full_engine_admitted: false,
        complete_consumer_admitted: false,
    })
}
pub fn validate(v: &MeaningV73) -> Result<(), String> {
    if *v != printed_meaning()? {
        return Err("V73 printed program changed".into());
    }
    Ok(())
}
