//! Selected printed rules; separate from public, Human and model admission.
use crate::card_def::{CardType, Keywords, Subtype, CARD_DEFS};
use crate::effect::{EffectCond, EffectOp};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use crate::trigger::TriggerCondition;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/slumbering-cerberus/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RootV1 {
    ControllerUntapStepRestriction,
    PrintedEndStepTrigger { trigger_index: u16 },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV1 {
    ControllerUntapStepRestriction {
        current_battlefield_definition_and_active_printed_abilities: bool,
        current_controller_step_not_owner_or_original_caster: bool,
        does_not_prevent_effect_untaps: bool,
        skipped_step_untap_does_not_remove_stun: bool,
        departure_definition_change_or_suppression_ends_restriction: bool,
    },
    PrintedEndStepTrigger {
        beginning_of_each_players_end_step: bool,
        successful_creature_death_required_at_trigger_and_resolution: bool,
        any_controller_creature_death_counts: bool,
        graveyard_arrival_without_battlefield_death_does_not_count: bool,
        replaced_exile_is_not_death: bool,
        active_printed_front_face_battlefield_required_when_triggering: bool,
        captured_controller_and_exact_source_incarnation: bool,
        no_targets_or_payment: bool,
        same_incarnation_current_controller_or_ability_loss_does_not_cancel: bool,
        departed_or_reentered_source_is_not_rebound: bool,
        untap_only_if_currently_tapped_on_battlefield: bool,
        stun_removes_one_counter_instead_of_untapping: bool,
        already_untapped_source_keeps_stun: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub root: RootV1,
    pub root_sha256: String,
    pub rule: RuleV1,
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
pub fn producer_sha256() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("rules_meaning_cerberus_v1.rs"),
        include_str!("slumbering_cerberus_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn live_selector() -> Result<SelectorV1, ErrorV1> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err(ErrorV1::UnsupportedCatalog);
    }
    Ok(SelectorV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(selector: &SelectorV1, card_id: u16, root: RootV1) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Slumbering Cerberus" || !def.is_executable() {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    let expected = EffectOp::Conditional {
        cond: EffectCond::CreatureDiedThisTurn,
        then: Box::new(EffectOp::StandardV1(
            crate::standard_cards_v1::StandardOpV1::UntapCapturedSourceV1,
        )),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    };
    let triggers = crate::trigger::triggers_for(card_id);
    if def.cost
        != (Cost {
            generic: 1,
            pips: &[Pip::Colored(ManaColor::R)],
            x_count: 0,
        })
        || def.colors != [ManaColor::R]
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Dog]
        || (def.power, def.toughness) != (Some(4), Some(2))
        || def.keywords != Keywords::NONE
        || (def.spell_effect)().is_some()
        || !def.activated_abilities.is_empty()
        || triggers.len() != 1
        || triggers[0].condition != TriggerCondition::BeginningAnyEndStepIfCreatureDied
        || triggers[0].home_zone != Zone::Battlefield
        || triggers[0].face_index != 0
        || triggers[0].intervening_if_kicked
        || triggers[0].intervening_if_controls_another_source_card
        || (triggers[0].effect)() != expected
    {
        return Err(ErrorV1::WrongRoot);
    }
    let rule = match root {
        RootV1::ControllerUntapStepRestriction => RuleV1::ControllerUntapStepRestriction {
            current_battlefield_definition_and_active_printed_abilities: true,
            current_controller_step_not_owner_or_original_caster: true,
            does_not_prevent_effect_untaps: true,
            skipped_step_untap_does_not_remove_stun: true,
            departure_definition_change_or_suppression_ends_restriction: true,
        },
        RootV1::PrintedEndStepTrigger { trigger_index: 0 } => RuleV1::PrintedEndStepTrigger {
            beginning_of_each_players_end_step: true,
            successful_creature_death_required_at_trigger_and_resolution: true,
            any_controller_creature_death_counts: true,
            graveyard_arrival_without_battlefield_death_does_not_count: true,
            replaced_exile_is_not_death: true,
            active_printed_front_face_battlefield_required_when_triggering: true,
            captured_controller_and_exact_source_incarnation: true,
            no_targets_or_payment: true,
            same_incarnation_current_controller_or_ability_loss_does_not_cancel: true,
            departed_or_reentered_source_is_not_rebound: true,
            untap_only_if_currently_tapped_on_battlefield: true,
            stun_removes_one_counter_instead_of_untapping: true,
            already_untapped_source_keeps_stun: true,
        },
        _ => return Err(ErrorV1::WrongRoot),
    };
    let bytes = serde_json::to_vec(&(
        card_id,
        root,
        def.cost,
        def.types,
        def.subtypes,
        def.power,
        def.toughness,
        &expected,
        &rule,
    ))
    .expect("selected root serializes");
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        root,
        root_sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        rule,
        public_human_sampler_and_model_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id, record.root)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_identity_and_frozen_refusal_precede_definition_lookup() {
        let Ok(selector) = live_selector() else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        for field in 0..3 {
            let mut wrong = selector.clone();
            match field {
                0 => wrong.schema.push('x'),
                1 => wrong.card_db_hash ^= 1,
                _ => wrong.producer_sha256.push('x'),
            }
            assert_eq!(
                meaning(&wrong, u16::MAX, RootV1::ControllerUntapStepRestriction),
                Err(ErrorV1::WrongSelector)
            );
        }
        assert_eq!(
            meaning(
                &selector,
                crate::card_def::card_id_by_name("Lightning Bolt").unwrap(),
                RootV1::ControllerUntapStepRestriction
            ),
            Err(ErrorV1::UnsupportedDefinition)
        );
    }
}
