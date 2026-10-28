//! Exact closed printed Genesis Wave program. Partial metadata grants no admission.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/genesis-wave/v69";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV69 {
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
pub enum InstructionV69 {
    AnnounceNormalHandSorceryXPlusGGGOrAuthenticGrantedCastMethod,
    KeepNativeCastPermissionAdditionalCostsAndResponseWindows,
    WithoutPayingManaCostForcesXZero,
    CopiesUseOriginalXWithoutBeingCastOrPayingAgain,
    UseThePaidOrCopiedNativeU16X,
    RevealUpToXTopCardsOfYourLibraryToBothPlayers,
    MayChooseAnyNumberOfRevealedPermanentCardsWithPrintedManaValueAtMostX,
    KeepChosenSubsetInOriginalRevealOrder,
    ChooseLegalPreexistingAuraHostsWithoutTargeting,
    AuraWithoutLegalHostCannotEnter,
    PutAllChosenEnteringPermanentsOntoBattlefieldSimultaneously,
    CompleteNativeAsEntersChoicesAndEntryReplacements,
    PutEveryRevealedCardNotEnteredOntoItsActualGraveyardReplacementPath,
    OrderOnlyActualGraveyardEntrantsAndCompleteMandatoryShuffles,
    FinishActualSpellDepartureBeforePlacingEntryTriggers,
    CounteredBeforeResolutionDoesNotReveal,
    PhysicalFlashbackExilesAndVirtualSpellCopiesCease,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV69 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV69,
    pub ordered_rules: Vec<InstructionV69>,
    pub full_engine_admitted: bool,
    pub owning_consumer_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn printed_meaning() -> Result<MeaningV69, String> {
    if !crate::effect::genesis_wave_v1::definition(487) {
        return Err("V69 requires the exact native Partial Genesis Wave definition".into());
    }
    use InstructionV69::*;
    Ok(MeaningV69 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        card_id: 487,
        printed: PrintedV69 {
            generic_mana: 0,
            colored_mana: vec![crate::mana::ManaColor::G; 3],
            mana_value: 3,
            instant: false,
            targets: 0,
            x_count: 1,
            types: vec![crate::card_def::CardType::Sorcery],
            subtypes: vec![],
            colors: vec![crate::mana::ManaColor::G],
            legendary: false,
            power: None,
            toughness: None,
            keyword_bits: 0,
        },
        ordered_rules: vec![
            AnnounceNormalHandSorceryXPlusGGGOrAuthenticGrantedCastMethod,
            KeepNativeCastPermissionAdditionalCostsAndResponseWindows,
            WithoutPayingManaCostForcesXZero,
            CopiesUseOriginalXWithoutBeingCastOrPayingAgain,
            UseThePaidOrCopiedNativeU16X,
            RevealUpToXTopCardsOfYourLibraryToBothPlayers,
            MayChooseAnyNumberOfRevealedPermanentCardsWithPrintedManaValueAtMostX,
            KeepChosenSubsetInOriginalRevealOrder,
            ChooseLegalPreexistingAuraHostsWithoutTargeting,
            AuraWithoutLegalHostCannotEnter,
            PutAllChosenEnteringPermanentsOntoBattlefieldSimultaneously,
            CompleteNativeAsEntersChoicesAndEntryReplacements,
            PutEveryRevealedCardNotEnteredOntoItsActualGraveyardReplacementPath,
            OrderOnlyActualGraveyardEntrantsAndCompleteMandatoryShuffles,
            FinishActualSpellDepartureBeforePlacingEntryTriggers,
            CounteredBeforeResolutionDoesNotReveal,
            PhysicalFlashbackExilesAndVirtualSpellCopiesCease,
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
        include_str!("rules_meaning_genesis_wave_v69.rs"),
        crate::effect::genesis_wave_v1::source_sha256_v1(),
    ] {
        h.update((source.len() as u64).to_le_bytes());
        h.update(source.as_bytes());
    }
    format!("{:x}", h.finalize())
}
pub fn validate(value: &MeaningV69) -> Result<(), String> {
    if *value != printed_meaning()? {
        return Err("V69 printed ordered meaning changed".into());
    }
    Ok(())
}
