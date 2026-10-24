//! Closed FDN243 meaning. Partial source preparation is not runtime admission.
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/muldrotha-gravetide/v71";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV71 {
    pub generic_mana: u8,
    pub colored_mana: Vec<crate::mana::ManaColor>,
    pub mana_value: u16,
    pub types: Vec<crate::card_def::CardType>,
    pub subtype_ids: Vec<u16>,
    pub legendary: bool,
    pub power: i32,
    pub toughness: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionV71 {
    CastCreatureForThreeBGUWithOrdinaryTimingCostsTargetsAndResponses,
    AbilityRequiresThisOriginalBattlefieldIncarnationAndActivePrintedAbilities,
    OnlyDuringYourOwnTurnYouMayPlayPermanentCardsFromYourOwnGraveyard,
    ChooseOrdinaryRouteOrOneSpecificOriginalSourceAndPermanentTypePermission,
    OnePlayOfEachPermanentTypePerSourceIncarnationPerTurn,
    MultipleSourcesHaveIndependentBudgets,
    LandUsesOnlyLandPermissionAndStillRequiresAnAvailableLandPlay,
    SpellFormMustBePermanentAndHaveTheChosenNonlandPermanentType,
    NormalAdventureCreatureIsPermittedButNonpermanentAdventureAndOmenAreNot,
    BestowUsesOnlyTheFinalEnchantmentTypeAndRetainsOrdinaryAuraRules,
    PayAllOrdinaryAndAdditionalCostsAndKeepCostsIfCountered,
    ConsumeTheChosenTypeOnlyWhenActualLandPlayOrSpellCastingCompletes,
    CounteringDoesNotRefundTheConsumedType,
    SourceDepartureReentryStartsANewIncarnationBudget,
    ControlChangeDoesNotResetTheSameIncarnationBudget,
    NewTurnResetsBudgetsEvenForExtraTurns,
    PermissionAndFinalizedSpellRetainOriginalCardAndSourceIncarnations,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV71 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub card_id: u16,
    pub printed: PrintedV71,
    pub ordered_rules: Vec<InstructionV71>,
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
            include_str!("rules_meaning_muldrotha_v71.rs"),
            include_str!("engine/muldrotha_v1.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
    .clone()
}
pub fn printed_meaning() -> Result<MeaningV71, String> {
    use crate::{
        card_def::{CardType, Subtype},
        mana::ManaColor,
    };
    if !crate::engine::muldrotha_v1::definition(489) {
        return Err("V71 exact Muldrotha absent".into());
    }
    use InstructionV71::*;
    Ok(MeaningV71 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        card_id: 489,
        printed: PrintedV71 {
            generic_mana: 3,
            colored_mana: vec![ManaColor::B, ManaColor::G, ManaColor::U],
            mana_value: 6,
            types: vec![CardType::Creature],
            subtype_ids: vec![Subtype::Elemental as u16, Subtype::Avatar as u16],
            legendary: true,
            power: 6,
            toughness: 6,
        },
        ordered_rules: vec![
            CastCreatureForThreeBGUWithOrdinaryTimingCostsTargetsAndResponses,
            AbilityRequiresThisOriginalBattlefieldIncarnationAndActivePrintedAbilities,
            OnlyDuringYourOwnTurnYouMayPlayPermanentCardsFromYourOwnGraveyard,
            ChooseOrdinaryRouteOrOneSpecificOriginalSourceAndPermanentTypePermission,
            OnePlayOfEachPermanentTypePerSourceIncarnationPerTurn,
            MultipleSourcesHaveIndependentBudgets,
            LandUsesOnlyLandPermissionAndStillRequiresAnAvailableLandPlay,
            SpellFormMustBePermanentAndHaveTheChosenNonlandPermanentType,
            NormalAdventureCreatureIsPermittedButNonpermanentAdventureAndOmenAreNot,
            BestowUsesOnlyTheFinalEnchantmentTypeAndRetainsOrdinaryAuraRules,
            PayAllOrdinaryAndAdditionalCostsAndKeepCostsIfCountered,
            ConsumeTheChosenTypeOnlyWhenActualLandPlayOrSpellCastingCompletes,
            CounteringDoesNotRefundTheConsumedType,
            SourceDepartureReentryStartsANewIncarnationBudget,
            ControlChangeDoesNotResetTheSameIncarnationBudget,
            NewTurnResetsBudgetsEvenForExtraTurns,
            PermissionAndFinalizedSpellRetainOriginalCardAndSourceIncarnations,
        ],
        full_engine_admitted: false,
        owning_consumer_admitted: false,
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(v: &MeaningV71) -> Result<(), String> {
    if *v != printed_meaning()? {
        return Err("V71 printed meaning changed".into());
    }
    Ok(())
}
