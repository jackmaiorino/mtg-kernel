//! Selected printed meanings only. Frozen observations, vectors and models remain unadmitted.
use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::EffectOp;
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use crate::trigger::{self, TriggerCondition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/authority-of-the-consuls/v1";
pub const SCOPE: &str = "pauper+fdn/limited-without-standard";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub scope: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV1 {
    OpponentCreatureEntryTappedReplacement,
    OpponentCreatureEntryGainLifeTrigger,
}

/// Closed, ordered executable facts. There are no caller-supplied programs or
/// model-vector aliases. Each root reconstructs its complete distinct list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FactV1 {
    ActivePrintedBattlefieldFace { face_index: u8 },
    ActualCreatureEntryFromZoneChangeOrCreateToken,
    EntryControllerOpposesObserverControllerNotPhysicalOwner,
    PreEntryObserverSnapshotForSimultaneousReplacement,
    EntersTappedBeforeEntryTriggersAndStateBasedActions,
    DoesNotTargetOrUseTheStack,
    NoLaterRetapAndStopsAffectingNewEntriesAfterObserverDeparture,
    PostEntryObserverSnapshotForSimultaneousTrigger,
    IndependentCommittedEntryAndOriginalObserverIncarnations,
    IndependentOriginalObserverDefinitionOwnerAndController,
    OneMandatoryTriggerPerActiveObserverPerCreatureEntry,
    UntargetedGainThroughOrdinaryLifeGainPipeline { amount: u8 },
    GainRecipientIsOriginalCapturedTriggerController,
    NoInterveningConditionOrResolutionCreatureRecheck,
    SourceAndEntrantDepartureReentryOrControlChangeDoNotChangeGain,
    OrdinaryApnapPlacementAndPriority,
    LegendWaitingReturnAndResolutionPlayDeferredOwners,
    ExactOnceCollectionAndFullFinalizedStackPublication,
    ActualGainOrCounterOperationBindsCompletion,
    ReadonlyRestoreRefusalForMissingReplayedOrChangedDispatch,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardMeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub printed_name: String,
    pub printed_cost: String,
    pub printed_mana_value: u16,
    pub ordinary_untargeted_enchantment_spell: bool,
    pub root: RootV1,
    pub trigger_index: Option<u16>,
    pub root_sha256: String,
    pub facts: Vec<FactV1>,
    pub public_human_sampler_and_model_admitted: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    WrongRoot,
    IdentityMismatch,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut digest = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_authority_v1.rs"),
        include_str!("authority_of_the_consuls_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("../build.rs"),
    ] {
        digest.update([0]);
        digest.update(source.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn live_selector_v1() -> Result<SelectorV1, ErrorV1> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV1::UnsupportedCatalog);
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        scope: SCOPE.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    })
}
pub fn meaning_v1(
    selector: &SelectorV1,
    card_id: u16,
    root: RootV1,
) -> Result<CardMeaningV1, ErrorV1> {
    if selector != &live_selector_v1()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Authority of the Consuls"
        || def.object_name != def.name
        || !def.is_executable()
        || def.is_token
        || def.cost.generic != 0
        || def.cost.pips != [Pip::Colored(ManaColor::W)]
        || def.mana_value != 1
        || def.colors != [ManaColor::W]
        || def.types != [CardType::Enchantment]
        || !def.subtypes.is_empty()
        || !def.supertypes.is_empty()
        || def.power.is_some()
        || def.toughness.is_some()
        || def.starting_loyalty.is_some()
        || def.is_land
        || !def.produces_mana.is_empty()
        || (def.mana_ability)().is_some()
        || def.generic_cost_reduction.is_some()
        || def.ward_cost.is_some()
        || def.spell_cannot_be_countered
        || def.records_mana_spent
        || def.target_spec != TargetSpec::None
        || def.keywords != Keywords::NONE
        || (def.spell_effect)().is_some()
        || !def.activated_abilities.is_empty()
        || def.alt_cost.is_some()
        || def.additional_cost.is_some()
        || def.kicker_cost.is_some()
        || def.flashback.is_some()
        || def.escape.is_some()
        || def.plot_cost.is_some()
        || def.madness_cost.is_some()
        || def.mode2.is_some()
        || def.mode3.is_some()
    {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let abilities = trigger::triggers_for(card_id);
    let [ability] = abilities else {
        return Err(ErrorV1::WrongRoot);
    };
    let program = (ability.effect)();
    if ability.condition != TriggerCondition::OpponentCreatureEntersV1
        || ability.home_zone != Zone::Battlefield
        || ability.face_index != 0
        || ability.intervening_if_kicked
        || ability.intervening_if_controls_another_source_card
        || program != EffectOp::BindAuthorityEntryGainLifeV1
        || trigger::target_spec_for_trigger(card_id, &program) != Some(TargetSpec::None)
    {
        return Err(ErrorV1::WrongRoot);
    }
    let (trigger_index, facts) = match root {
        RootV1::OpponentCreatureEntryTappedReplacement => (
            None,
            vec![
                FactV1::ActivePrintedBattlefieldFace { face_index: 0 },
                FactV1::ActualCreatureEntryFromZoneChangeOrCreateToken,
                FactV1::EntryControllerOpposesObserverControllerNotPhysicalOwner,
                FactV1::PreEntryObserverSnapshotForSimultaneousReplacement,
                FactV1::EntersTappedBeforeEntryTriggersAndStateBasedActions,
                FactV1::DoesNotTargetOrUseTheStack,
                FactV1::NoLaterRetapAndStopsAffectingNewEntriesAfterObserverDeparture,
            ],
        ),
        RootV1::OpponentCreatureEntryGainLifeTrigger => (
            Some(0),
            vec![
                FactV1::ActivePrintedBattlefieldFace { face_index: 0 },
                FactV1::ActualCreatureEntryFromZoneChangeOrCreateToken,
                FactV1::EntryControllerOpposesObserverControllerNotPhysicalOwner,
                FactV1::PostEntryObserverSnapshotForSimultaneousTrigger,
                FactV1::IndependentCommittedEntryAndOriginalObserverIncarnations,
                FactV1::IndependentOriginalObserverDefinitionOwnerAndController,
                FactV1::OneMandatoryTriggerPerActiveObserverPerCreatureEntry,
                FactV1::UntargetedGainThroughOrdinaryLifeGainPipeline { amount: 1 },
                FactV1::GainRecipientIsOriginalCapturedTriggerController,
                FactV1::NoInterveningConditionOrResolutionCreatureRecheck,
                FactV1::SourceAndEntrantDepartureReentryOrControlChangeDoNotChangeGain,
                FactV1::OrdinaryApnapPlacementAndPriority,
                FactV1::LegendWaitingReturnAndResolutionPlayDeferredOwners,
                FactV1::ExactOnceCollectionAndFullFinalizedStackPublication,
                FactV1::ActualGainOrCounterOperationBindsCompletion,
                FactV1::ReadonlyRestoreRefusalForMissingReplayedOrChangedDispatch,
            ],
        ),
    };
    let bytes = serde_json::to_vec(&(
        (card_id, root, trigger_index),
        (
            def.name,
            def.object_name,
            "{W}",
            def.mana_value,
            def.types,
            def.colors,
            def.subtypes,
            def.supertypes,
        ),
        (
            format!("{:?}", ability.condition),
            ability.home_zone,
            ability.face_index,
            ability.intervening_if_kicked,
            ability.intervening_if_controls_another_source_card,
            program,
        ),
        &facts,
    ))
    .expect("selected definition facts serialize");
    Ok(CardMeaningV1 {
        selector: selector.clone(),
        card_id,
        printed_name: def.name.into(),
        printed_cost: "{W}".into(),
        printed_mana_value: def.mana_value,
        ordinary_untargeted_enchantment_spell: true,
        root,
        trigger_index,
        root_sha256: digest(&bytes),
        facts,
        public_human_sampler_and_model_admitted: false,
    })
}
pub fn validate_meaning_v1(meaning: &CardMeaningV1) -> Result<(), ErrorV1> {
    if *meaning != meaning_v1(&meaning.selector, meaning.card_id, meaning.root)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_catalogs_and_foreign_definitions_refuse_exact_authority_selection() {
        if let Ok(selector) = live_selector_v1() {
            let foreign = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
            for root in [
                RootV1::OpponentCreatureEntryTappedReplacement,
                RootV1::OpponentCreatureEntryGainLifeTrigger,
            ] {
                assert_eq!(
                    meaning_v1(&selector, foreign, root),
                    Err(ErrorV1::UnsupportedDefinition)
                );
                assert_eq!(
                    meaning_v1(&selector, u16::MAX, root),
                    Err(ErrorV1::UnsupportedDefinition)
                );
                for field in 0..4 {
                    let mut forged = selector.clone();
                    match field {
                        0 => forged.schema.push('x'),
                        1 => forged.scope.push('x'),
                        2 => forged.card_db_hash ^= 1,
                        _ => forged.producer_sha256.push('x'),
                    }
                    assert_eq!(
                        meaning_v1(&forged, foreign, root),
                        Err(ErrorV1::WrongSelector)
                    );
                }
            }
        } else {
            assert_eq!(live_selector_v1(), Err(ErrorV1::UnsupportedCatalog));
        }
        assert_eq!(producer_sha256_v1().len(), 64);
    }
}
