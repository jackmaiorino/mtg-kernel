//! Exact printed Herald meaning, separate from frozen vector representations.
use crate::card_def::{
    CardType, GameOutcomeProtectionDefV1, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "mtg-kernel/rules-meaning/game-outcome-protection/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV1 {
    pub schema: String,
    pub card_db_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub selector: SelectorV1,
    pub card_id: u16,
    pub printed_cost: String,
    pub flash: bool,
    pub flying: bool,
    pub controller_cannot_lose_game: bool,
    pub opponents_cannot_win_game: bool,
    pub current_controller_and_active_battlefield_printed_ability_required: bool,
    pub applies_to_life_poison_and_empty_draw_losses: bool,
    pub protection_sampled_before_simultaneous_sba_departures: bool,
    pub protected_empty_draw_attempt_expires_at_its_sba_check: bool,
    pub life_and_poison_conditions_persist_after_source_leaves: bool,
    pub no_target_payment_choice_or_saved_duration: bool,
    pub model_and_sampler_admitted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    UnsupportedCatalog,
    WrongSelector,
    UnsupportedDefinition,
    IdentityMismatch,
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
    })
}

pub fn meaning(selector: &SelectorV1, card_id: u16) -> Result<MeaningV1, ErrorV1> {
    if *selector != live_selector()? {
        return Err(ErrorV1::WrongSelector);
    }
    let def = CARD_DEFS
        .get(usize::from(card_id))
        .ok_or(ErrorV1::UnsupportedDefinition)?;
    if def.name != "Herald of Eternal Dawn"
        || !def.capability.is_fully_supported()
        || def.cost
            != (Cost {
                generic: 4,
                pips: &[Pip::Colored(ManaColor::W); 3],
                x_count: 0,
            })
        || def.mana_value != 7
        || def.types != [CardType::Creature]
        || def.subtypes != [Subtype::Angel]
        || !def.supertypes.is_empty()
        || def.colors != [ManaColor::W]
        || (def.power, def.toughness) != (Some(6), Some(6))
        || def.keywords != (Keywords::FLASH | Keywords::FLYING)
        || def.game_outcome_protection_v1
            != Some(GameOutcomeProtectionDefV1::ControllerCannotLoseAndOpponentsCannotWin)
        || def.target_spec != TargetSpec::None
        || (def.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !def.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(card_id).is_empty()
        || def.is_token
        || def.is_land
        || def.transform_face.is_some()
        || def.alt_cost.is_some()
        || def.flashback.is_some()
        || def.adventure.is_some()
        || def.ward_cost.is_some()
        || def.equipment.is_some()
        || (def.mana_ability)().is_some()
        || def.mana_ability_def.is_some()
        || !def.additional_mana_abilities.is_empty()
    {
        return Err(ErrorV1::UnsupportedDefinition);
    }
    Ok(MeaningV1 {
        selector: selector.clone(),
        card_id,
        printed_cost: "{4}{W}{W}{W}".into(),
        flash: true,
        flying: true,
        controller_cannot_lose_game: true,
        opponents_cannot_win_game: true,
        current_controller_and_active_battlefield_printed_ability_required: true,
        applies_to_life_poison_and_empty_draw_losses: true,
        protection_sampled_before_simultaneous_sba_departures: true,
        protected_empty_draw_attempt_expires_at_its_sba_check: true,
        life_and_poison_conditions_persist_after_source_leaves: true,
        no_target_payment_choice_or_saved_duration: true,
        model_and_sampler_admitted: false,
    })
}

pub fn validate(record: &MeaningV1) -> Result<(), ErrorV1> {
    if *record != meaning(&record.selector, record.card_id)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrong_selected_catalog_refuses_before_definition_lookup() {
        let Ok(mut selector) = live_selector() else {
            assert_eq!(live_selector(), Err(ErrorV1::UnsupportedCatalog));
            return;
        };
        selector.card_db_hash ^= 1;
        assert_eq!(meaning(&selector, u16::MAX), Err(ErrorV1::WrongSelector));
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[ignore = "requires genuine Herald registration and native qualification"]
    fn actual_registered_printed_prohibition_rederives_every_fact() {
        let id = crate::card_def::card_id_by_name("Herald of Eternal Dawn").unwrap();
        let record = meaning(&live_selector().unwrap(), id).unwrap();
        validate(&record).unwrap();
        for field in 0..5 {
            let mut bad = record.clone();
            match field {
                0 => bad.controller_cannot_lose_game = false,
                1 => bad.opponents_cannot_win_game = false,
                2 => bad.protected_empty_draw_attempt_expires_at_its_sba_check = false,
                3 => bad.printed_cost = "{7}".into(),
                _ => bad.model_and_sampler_admitted = true,
            }
            assert_eq!(validate(&bad), Err(ErrorV1::IdentityMismatch));
        }
    }
}
