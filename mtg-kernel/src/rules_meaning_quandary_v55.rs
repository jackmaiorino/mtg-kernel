//! Exact optional discard, ordered unpaid branch and captured-controller meaning.
pub use crate::optional_trigger_source_v1::{
    CastDepartureV1, CastInstructionV1, CastV1, PrintedV1,
};
use crate::{
    card_def::CardCapability,
    effect::{EffectOp, PlayerRef},
    optional_trigger_source_v1::{self as source, Kind},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/quandary/v55";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV55 {
    OpponentGenuinelyCastsSpell {
        copies_are_not_casts: bool,
        printed_battlefield_ability_required: bool,
        retain_original_source_incarnation_and_trigger_controller: bool,
        ordered_resolution: Vec<PaymentInstructionV55>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PaymentInstructionV55 {
    OriginalCastingOpponentMaySelectZeroOrOneFromExactCurrentPrivateHand,
    EmptyHandForcesUnpaidBranch,
    SelectedCardUsesActualDiscardEventAndReplacements,
    OtherwiseOriginalCastingOpponentLosesLife { amount: i32 },
    ContinueOnlyOriginalOrderedSiblings,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV55 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_id: u16,
    pub capability: CardCapability,
    pub printed: PrintedV1,
    pub cast: CastV1,
    pub ordered_rules: Vec<InstructionV55>,
    pub root_sha256: String,
    pub model_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| source::producer(SCHEMA)).clone()
}
pub fn printed_meaning(card_id: u16) -> Result<MeaningV55, String> {
    let d = source::printing(card_id, Kind::Quandary)?;
    let [trigger] = crate::trigger::triggers_for(card_id) else {
        unreachable!()
    };
    let EffectOp::MayDiscardCardOtherwiseV1 {
        player: PlayerRef::Opponent,
        unpaid,
    } = (trigger.effect)()
    else {
        return Err("Quandary payer/root changed".into());
    };
    let EffectOp::LoseLife {
        player: PlayerRef::Opponent,
        amount,
    } = *unpaid
    else {
        return Err("Quandary unpaid program changed".into());
    };
    if amount != 5 {
        return Err("Quandary life amount changed".into());
    }
    let ordered_rules = vec![InstructionV55::OpponentGenuinelyCastsSpell {
        copies_are_not_casts: true,
        printed_battlefield_ability_required: true,
        retain_original_source_incarnation_and_trigger_controller: true,
        ordered_resolution: vec![
            PaymentInstructionV55::OriginalCastingOpponentMaySelectZeroOrOneFromExactCurrentPrivateHand,
            PaymentInstructionV55::EmptyHandForcesUnpaidBranch,
            PaymentInstructionV55::SelectedCardUsesActualDiscardEventAndReplacements,
            PaymentInstructionV55::OtherwiseOriginalCastingOpponentLosesLife { amount },
            PaymentInstructionV55::ContinueOnlyOriginalOrderedSiblings,
        ],
    }];
    let printed = source::printed(d);
    let cast = source::cast();
    let root_sha256 = source::digest(&(
        SCHEMA,
        card_id,
        d.capability,
        &printed,
        &cast,
        &ordered_rules,
    ))?;
    Ok(MeaningV55 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_id,
        capability: d.capability,
        printed,
        cast,
        ordered_rules,
        root_sha256,
        model_admitted: false,
    })
}
pub fn validate(value: &MeaningV55) -> Result<(), String> {
    if *value == printed_meaning(value.card_id)? {
        Ok(())
    } else {
        Err("Quandary typed meaning changed".into())
    }
}
