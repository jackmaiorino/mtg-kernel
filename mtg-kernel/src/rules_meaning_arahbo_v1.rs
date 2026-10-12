//! Exact selected roots. Frozen RulesMeaning vectors retain Opaque leaves.
use crate::card_def::{CardType, Keywords, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{ManaColor, Pip};
use crate::state::Zone;
use crate::trigger::{self, TriggerCondition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/arahbo/v1";
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
    OtherControlledCatCreaturesGetPlusOne,
    ThisOrOtherControlledNontokenCatPermanentCreatesCat,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactV1 {
    ActivePrintedFrontFaceBattlefieldSource,
    UntargetedOrdinaryLegendaryCreatureSpell,
    OtherCurrentControlledEffectiveCatCreatures,
    ContinuousPlusOnePowerAndToughness,
    ThisSourceEntryIncludesTokenCopy,
    OtherEntryIsNontokenControlledCatPermanentNotCreatureOnly,
    CompleteSimultaneousBoardCountersAndAttachmentsBeforeObservation,
    CaptureBeforeStateBasedActions,
    OriginalControllerAndSourceIncarnation,
    NoLaterEntrantOrSourcePresenceOrControllerRecheck,
    MandatoryPendingLegendDeferredOrUniquePublishedOwner,
    OrdinaryApnapAndPriority,
    GenuinePreRemovalResolveOrCounterOwner,
    OneActualWhiteOneOneCatTokenCreation,
    RecipientOriginalCapturedController,
    ActualCreationOrIndependentCounterCompletion,
    MissingReplayedChangedOwnersRefuseReadonly,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardMeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub printed_name: String,
    pub printed_cost: String,
    pub printed_mana_value: u16,
    pub root: RootV1,
    pub trigger_index: Option<u16>,
    pub root_sha256: String,
    pub facts: Vec<FactV1>,
    pub model_and_sampler_admitted: bool,
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
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_arahbo_v1.rs"),
        include_str!("arahbo_v1.rs"),
        include_str!("garruks_uprising_v1.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("linked_aura_return_v1.rs"),
        include_str!("banishing_light_v1.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("trigger.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("fdn_raid_v1.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
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
    if def.name != "Arahbo, the First Fang"
        || def.object_name != def.name
        || !def.is_executable()
        || def.is_token
        || def.is_land
        || def.cost.generic != 2
        || def.cost.x_count != 0
        || def.cost.pips != [Pip::Colored(ManaColor::W)]
        || def.mana_value != 3
        || def.colors != [ManaColor::W]
        || def.types != [CardType::Creature]
        || def.subtypes
            != [
                crate::card_def::Subtype::Cat,
                crate::card_def::Subtype::Avatar,
            ]
        || def.supertypes != [crate::card_def::Supertype::Legendary]
        || def.power != Some(2)
        || def.toughness != Some(2)
        || def.starting_loyalty.is_some()
        || !def.produces_mana.is_empty()
        || (def.mana_ability)().is_some()
        || def.target_spec != TargetSpec::None
        || def.keywords != Keywords::NONE
        || def.ward_cost.is_some()
        || def.generic_cost_reduction.is_some()
        || def.spell_cannot_be_countered
        || def.records_mana_spent
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
        || def.enters_with_plus_one_counters.is_some()
        || def.controlled_counter_keyword.is_some()
        || !def.mana_ability_choices.is_empty()
        || def.enters_battlefield_tapped
        || def.mana_ability_def.is_some()
        || def.minimum_blockers != 1
        || def.omen.is_some()
        || def.mana_ability_includes_chosen_color
        || def.as_enters_choose_color_other_than.is_some()
        || !def.additional_mana_abilities.is_empty()
        || def.enters_battlefield_tapped_unless.is_some()
        || def.transform_face.is_some()
        || def.saga.is_some()
        || def.optional_additional_cost.is_some()
        || def.changeling
        || def.bestow.is_some()
        || def.delve
        || def.adventure.is_some()
        || def.cant_be_blocked_by_monarchs_creatures
        || def.conditional_tap_yield.is_some()
        || def.enters_tapped_unless_controller.is_some()
        || !def.additional_mana_ability_conditions.is_empty()
        || !def.restricted_mana_abilities.is_empty()
        || def.animation.is_some()
        || !def.activated_ability_generic_reductions.is_empty()
        || (def.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    crate::arahbo_v1::cat_token_definition().map_err(|_| ErrorV1::UnsupportedDefinition)?;
    let abilities = trigger::triggers_for(card_id);
    if abilities.len() != 1 {
        return Err(ErrorV1::WrongRoot);
    }
    let ability = &abilities[0];
    if ability.condition != TriggerCondition::ArahboThisOrOtherNontokenCatEntryV1
        || (ability.effect)() != EffectOp::BindArahboCatEntryV1
        || ability.home_zone != Zone::Battlefield
        || ability.face_index != 0
        || ability.intervening_if_kicked
        || ability.intervening_if_controls_another_source_card
        || trigger::target_spec_for_trigger(card_id, &(ability.effect)()) != Some(TargetSpec::None)
    {
        return Err(ErrorV1::WrongRoot);
    }
    let boost =
        crate::engine::static_controlled_subtype_boost_for(def.name).ok_or(ErrorV1::WrongRoot)?;
    if boost.subtype != crate::card_def::Subtype::Cat
        || !boost.exclude_source
        || boost.power != 1
        || boost.toughness != 1
    {
        return Err(ErrorV1::WrongRoot);
    }
    use FactV1::*;
    let (trigger_index, facts) = match root {
        RootV1::OtherControlledCatCreaturesGetPlusOne => (
            None,
            vec![
                ActivePrintedFrontFaceBattlefieldSource,
                OtherCurrentControlledEffectiveCatCreatures,
                ContinuousPlusOnePowerAndToughness,
            ],
        ),
        RootV1::ThisOrOtherControlledNontokenCatPermanentCreatesCat => (
            Some(0),
            vec![
                ActivePrintedFrontFaceBattlefieldSource,
                UntargetedOrdinaryLegendaryCreatureSpell,
                ThisSourceEntryIncludesTokenCopy,
                OtherEntryIsNontokenControlledCatPermanentNotCreatureOnly,
                CompleteSimultaneousBoardCountersAndAttachmentsBeforeObservation,
                CaptureBeforeStateBasedActions,
                OriginalControllerAndSourceIncarnation,
                NoLaterEntrantOrSourcePresenceOrControllerRecheck,
                MandatoryPendingLegendDeferredOrUniquePublishedOwner,
                OrdinaryApnapAndPriority,
                GenuinePreRemovalResolveOrCounterOwner,
                OneActualWhiteOneOneCatTokenCreation,
                RecipientOriginalCapturedController,
                ActualCreationOrIndependentCounterCompletion,
                MissingReplayedChangedOwnersRefuseReadonly,
            ],
        ),
    };
    let programs = abilities
        .iter()
        .map(|ability| {
            (
                format!("{:?}", ability.condition),
                ability.home_zone,
                ability.face_index,
                ability.intervening_if_kicked,
                ability.intervening_if_controls_another_source_card,
                (ability.effect)(),
            )
        })
        .collect::<Vec<_>>();
    let root_sha256 = digest(
        &serde_json::to_vec(&(
            card_id,
            root,
            trigger_index,
            def.name,
            "{2}{W}",
            def.mana_value,
            def.types,
            def.colors,
            def.subtypes
                .iter()
                .map(|subtype| subtype.stable_id())
                .collect::<Vec<_>>(),
            "Legendary",
            def.power,
            def.toughness,
            programs,
            &facts,
        ))
        .expect("typed selected roots serialize"),
    );
    Ok(CardMeaningV1 {
        selector: selector.clone(),
        card_id,
        printed_name: def.name.into(),
        printed_cost: "{2}{W}".into(),
        printed_mana_value: def.mana_value,
        root,
        trigger_index,
        root_sha256,
        facts,
        model_and_sampler_admitted: false,
    })
}
pub fn validate_meaning_v1(meaning: &CardMeaningV1) -> Result<(), ErrorV1> {
    if meaning != &meaning_v1(&meaning.selector, meaning.card_id, meaning.root)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_selector_refuses_foreign_definition_and_default_standard() {
        match live_selector_v1() {
            Ok(selector) => {
                let id = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
                for root in [
                    RootV1::OtherControlledCatCreaturesGetPlusOne,
                    RootV1::ThisOrOtherControlledNontokenCatPermanentCreatesCat,
                ] {
                    assert_eq!(
                        meaning_v1(&selector, id, root),
                        Err(ErrorV1::UnsupportedDefinition)
                    );
                    let mut bad = selector.clone();
                    bad.card_db_hash ^= 1;
                    assert_eq!(meaning_v1(&bad, id, root), Err(ErrorV1::WrongSelector));
                }
            }
            Err(error) => assert_eq!(error, ErrorV1::UnsupportedCatalog),
        }
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    #[ignore = "requires genuine Arahbo registration; no fabricated ID or selector"]
    fn actual_two_roots_reconstruct_and_refuse_complete_fact_metadata_forgeries() {
        let id = crate::card_def::card_id_by_name("Arahbo, the First Fang").unwrap();
        let selector = live_selector_v1().unwrap();
        let mut hashes = Vec::new();
        for root in [
            RootV1::OtherControlledCatCreaturesGetPlusOne,
            RootV1::ThisOrOtherControlledNontokenCatPermanentCreatesCat,
        ] {
            let valid = meaning_v1(&selector, id, root).unwrap();
            validate_meaning_v1(&valid).unwrap();
            hashes.push(valid.root_sha256.clone());
            for case in 0..7 {
                let mut bad = valid.clone();
                match case {
                    0 => bad.facts.reverse(),
                    1 => {
                        bad.facts.pop();
                    }
                    2 => bad.printed_cost.clear(),
                    3 => bad.trigger_index = Some(9),
                    4 => bad.root_sha256.clear(),
                    5 => bad.selector.producer_sha256.clear(),
                    _ => bad.model_and_sampler_admitted = true,
                };
                assert!(validate_meaning_v1(&bad).is_err());
            }
        }
        assert_ne!(hashes[0], hashes[1]);
    }
}
