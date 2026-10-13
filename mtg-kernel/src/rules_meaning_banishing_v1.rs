//! Selected printed meanings only. Frozen observations, vectors and models remain unadmitted.
use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use crate::trigger::{self, TriggerCondition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/banishing-light/v1";
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
    OrdinaryPermanentSpell,
    PrintedEtb { trigger_index: u16 },
    ImmediateUntilSourceLeavesReturn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FactV1 {
    OrdinaryUntargetedPermanentSpell { printed_cost: String },
    EntersBattlefieldBeforeOneMandatoryTrigger,
    ActivePrintedFrontFaceAndGenuineZoneChangeOrTokenEntry,
    OriginalTriggerControllerSourceDefinitionOwnerAndIncarnation,
    MandatoryTargetOpponentNonlandPermanentAtPlacement,
    SourceAwareResolutionTargetRecheckProtectionHexproofAndWard,
    DepartedOrReenteredTargetIsNotRebound,
    SourceAlreadyLeftBeforeResolutionNeverExiles,
    ExactSuccessfulExileBindsPhysicalOwnerAndIncarnation,
    LaterSourceControlOrAbilityLossDoesNotEndInstalledDuration,
    ReturnImmediatelyOnOriginalIncarnationDepartureWithoutLeaveTrigger,
    SourceReentryDoesNotRenewOriginalDuration,
    ReturnUnderPhysicalOwnersControl,
    TokenCannotReturnAndDepartedExileRecipientIsNotRebound,
    AuraOwnerChoosesLegalUntargetedHostIncludingShroudAndHexproof,
    AuraNoLegalHostRemainsExiled,
    SimultaneousReturnsCaptureAllPreEntryMenusAndChooseApnap,
    AllRequiredOwnerAnswersPrecedeOneReturnEntryBatch,
    ActualOperationCostsEffectsAndRngResumeExactlyOnceAfterChoices,
    OrdinaryApnapPlacementPriorityAndCounterableEtb,
    IndependentOnceCollectionPublicationRemovalAndClosure,
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
        include_str!("rules_meaning_banishing_v1.rs"),
        include_str!("banishing_light_v1.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("linked_aura_return_v1.rs"),
        include_str!("standard_cards_v1.rs"),
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
    if def.name != "Banishing Light"
        || def.object_name != def.name
        || !def.is_executable()
        || def.is_token
        || def.cost.generic != 2
        || def.cost.x_count != 0
        || def.cost.pips != [Pip::Colored(ManaColor::W)]
        || def.mana_value != 3
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
        || (def.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
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
        || !def.additional_modes.is_empty()
        || def.kicked_target_spec.is_some()
        || def.equipment.is_some()
        || def.attachment.is_some()
        || def.transform_face.is_some()
        || def.saga.is_some()
        || def.optional_additional_cost.is_some()
        || def.changeling
        || def.bestow.is_some()
        || def.delve
        || def.adventure.is_some()
        || def.enters_with_plus_one_counters.is_some()
        || def.controlled_counter_keyword.is_some()
        || def.controlled_creature_grant.is_some()
        || !def.mana_ability_choices.is_empty()
        || def.mana_ability_def.is_some()
        || def.enters_battlefield_tapped
        || def.minimum_blockers > 1
        || def.omen.is_some()
        || def.mana_ability_includes_chosen_color
        || def.as_enters_choose_color_other_than.is_some()
        || !def.additional_mana_abilities.is_empty()
        || def.enters_battlefield_tapped_unless.is_some()
        || def.cant_be_blocked_by_monarchs_creatures
        || def.conditional_tap_yield.is_some()
        || def.enters_tapped_unless_controller.is_some()
        || !def.additional_mana_ability_conditions.is_empty()
        || !def.restricted_mana_abilities.is_empty()
        || def.animation.is_some()
        || !def.activated_ability_generic_reductions.is_empty()
    {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let [ability] = trigger::triggers_for(card_id) else {
        return Err(ErrorV1::WrongRoot);
    };
    let program = (ability.effect)();
    if ability.condition != TriggerCondition::Etb
        || ability.home_zone != Zone::Battlefield
        || ability.face_index != 0
        || ability.intervening_if_kicked
        || ability.intervening_if_controls_another_source_card
        || program != crate::banishing_light_v1::recipe()
        || trigger::target_spec_for_trigger(card_id, &program)
            != Some(TargetSpec::OpponentNonlandPermanent)
    {
        return Err(ErrorV1::WrongRoot);
    }
    let (trigger_index, facts) = match root {
        RootV1::OrdinaryPermanentSpell => (
            None,
            vec![
                FactV1::OrdinaryUntargetedPermanentSpell {
                    printed_cost: "{2}{W}".into(),
                },
                FactV1::EntersBattlefieldBeforeOneMandatoryTrigger,
            ],
        ),
        RootV1::PrintedEtb { trigger_index: 0 } => (
            Some(0),
            vec![
                FactV1::ActivePrintedFrontFaceAndGenuineZoneChangeOrTokenEntry,
                FactV1::OriginalTriggerControllerSourceDefinitionOwnerAndIncarnation,
                FactV1::MandatoryTargetOpponentNonlandPermanentAtPlacement,
                FactV1::SourceAwareResolutionTargetRecheckProtectionHexproofAndWard,
                FactV1::DepartedOrReenteredTargetIsNotRebound,
                FactV1::SourceAlreadyLeftBeforeResolutionNeverExiles,
                FactV1::ExactSuccessfulExileBindsPhysicalOwnerAndIncarnation,
                FactV1::OrdinaryApnapPlacementPriorityAndCounterableEtb,
                FactV1::IndependentOnceCollectionPublicationRemovalAndClosure,
            ],
        ),
        RootV1::ImmediateUntilSourceLeavesReturn => (
            None,
            vec![
                FactV1::LaterSourceControlOrAbilityLossDoesNotEndInstalledDuration,
                FactV1::ReturnImmediatelyOnOriginalIncarnationDepartureWithoutLeaveTrigger,
                FactV1::SourceReentryDoesNotRenewOriginalDuration,
                FactV1::ReturnUnderPhysicalOwnersControl,
                FactV1::TokenCannotReturnAndDepartedExileRecipientIsNotRebound,
                FactV1::AuraOwnerChoosesLegalUntargetedHostIncludingShroudAndHexproof,
                FactV1::AuraNoLegalHostRemainsExiled,
                FactV1::SimultaneousReturnsCaptureAllPreEntryMenusAndChooseApnap,
                FactV1::AllRequiredOwnerAnswersPrecedeOneReturnEntryBatch,
                FactV1::ActualOperationCostsEffectsAndRngResumeExactlyOnceAfterChoices,
            ],
        ),
        _ => return Err(ErrorV1::WrongRoot),
    };
    let bytes = serde_json::to_vec(&(
        (card_id, root, trigger_index),
        (
            def.name,
            def.object_name,
            "{2}{W}",
            def.mana_value,
            def.types,
            def.colors,
            def.subtypes,
            def.supertypes
                .iter()
                .map(|supertype| match supertype {
                    crate::card_def::Supertype::Basic => "Basic",
                    crate::card_def::Supertype::Snow => "Snow",
                    crate::card_def::Supertype::Legendary => "Legendary",
                })
                .collect::<Vec<_>>(),
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
        printed_cost: "{2}{W}".into(),
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
    fn exact_selector_and_absent_definition_refuse_without_fabricated_catalog() {
        let Ok(selector) = live_selector_v1() else {
            assert_eq!(live_selector_v1(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        let mut wrong = selector.clone();
        wrong.card_db_hash ^= 1;
        assert_eq!(
            meaning_v1(&wrong, u16::MAX, RootV1::OrdinaryPermanentSpell),
            Err(ErrorV1::WrongSelector)
        );
        let bolt = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
        assert_eq!(
            meaning_v1(&selector, bolt, RootV1::OrdinaryPermanentSpell),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_roots_reconstruct_all_metadata_and_ordered_facts() {
        let selector = live_selector_v1().unwrap();
        let id = crate::card_def::card_id_by_name("Banishing Light").unwrap();
        for root in [
            RootV1::OrdinaryPermanentSpell,
            RootV1::PrintedEtb { trigger_index: 0 },
            RootV1::ImmediateUntilSourceLeavesReturn,
        ] {
            let meaning = meaning_v1(&selector, id, root).unwrap();
            validate_meaning_v1(&meaning).unwrap();
            for field in 0..5 {
                let mut forged = meaning.clone();
                match field {
                    0 => forged.facts.reverse(),
                    1 => forged.facts.clear(),
                    2 => forged.printed_cost = "{W}".into(),
                    3 => forged.trigger_index = Some(7),
                    _ => forged.public_human_sampler_and_model_admitted = true,
                }
                assert_eq!(validate_meaning_v1(&forged), Err(ErrorV1::IdentityMismatch));
            }
        }
        assert_eq!(
            meaning_v1(&selector, id, RootV1::PrintedEtb { trigger_index: 1 }),
            Err(ErrorV1::WrongRoot)
        );
    }
}
