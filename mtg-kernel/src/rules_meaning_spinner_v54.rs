//! Ordered, definition-derived Spinner meaning. Integrity digests are not rules.
pub use crate::optional_trigger_source_v1::{
    CastDepartureV1, CastInstructionV1, CastV1, PrintedV1,
};
use crate::{
    card_def::{CardCapability, CardType},
    effect::{EffectOp, PlayerRef},
    optional_trigger_source_v1::{self as source, Kind},
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "rules-meaning/spinner/v54";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV54 {
    Reach,
    OtherControlledNontokenCreatureDies {
        exclude_source: bool,
        prebatch_battlefield_controller_and_incarnation: bool,
        printed_front_face_ability_required_at_event: bool,
        retain_original_trigger_controller: bool,
        optional_reveal: Vec<RevealInstructionV54>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RevealInstructionV54 {
    ControllerChoosesBeforeLibraryReadOrRng,
    DeclineDoesNothing,
    PublicRevealThroughFirstMatchingCardOrLibraryEnd { card_type: CardType },
    PutMatchingCardInControllersHandWithPublicIdentity,
    PutOnlyRevealedNonmatchesOnBottomInRandomOrder,
    PreserveUnrevealedSuffixAndInvalidateRandomizedBottomKnowledge,
    EmptyOrNoMatchDoesNotDraw,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV54 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_id: u16,
    pub capability: CardCapability,
    pub printed: PrintedV1,
    pub cast: CastV1,
    pub ordered_rules: Vec<InstructionV54>,
    pub root_sha256: String,
    pub model_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| source::producer(SCHEMA)).clone()
}
pub fn printed_meaning(card_id: u16) -> Result<MeaningV54, String> {
    let d = source::printing(card_id, Kind::Spinner)?;
    let [trigger] = crate::trigger::triggers_for(card_id) else {
        unreachable!()
    };
    let EffectOp::Choice {
        controller: PlayerRef::Controller,
        options,
    } = (trigger.effect)()
    else {
        return Err("Spinner optional controller/root changed".into());
    };
    let [EffectOp::Sequence(decline), EffectOp::RevealUntilCardTypeToHandRandomBottomV1 {
        player: PlayerRef::Controller,
        card_type,
    }] = options.as_slice()
    else {
        return Err("Spinner ordered options changed".into());
    };
    if !decline.is_empty() || *card_type != CardType::Creature {
        return Err("Spinner reveal predicate/decline changed".into());
    }
    let ordered_rules = vec![
        InstructionV54::Reach,
        InstructionV54::OtherControlledNontokenCreatureDies {
            exclude_source: true,
            prebatch_battlefield_controller_and_incarnation: true,
            printed_front_face_ability_required_at_event: true,
            retain_original_trigger_controller: true,
            optional_reveal: vec![
                RevealInstructionV54::ControllerChoosesBeforeLibraryReadOrRng,
                RevealInstructionV54::DeclineDoesNothing,
                RevealInstructionV54::PublicRevealThroughFirstMatchingCardOrLibraryEnd {
                    card_type: *card_type,
                },
                RevealInstructionV54::PutMatchingCardInControllersHandWithPublicIdentity,
                RevealInstructionV54::PutOnlyRevealedNonmatchesOnBottomInRandomOrder,
                RevealInstructionV54::PreserveUnrevealedSuffixAndInvalidateRandomizedBottomKnowledge,
                RevealInstructionV54::EmptyOrNoMatchDoesNotDraw,
            ],
        },
    ];
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
    Ok(MeaningV54 {
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
pub fn validate(value: &MeaningV54) -> Result<(), String> {
    if *value == printed_meaning(value.card_id)? {
        Ok(())
    } else {
        Err("Spinner typed meaning changed".into())
    }
}
