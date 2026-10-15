//! Explicit Herald member in the shared canonical Human projection.
use super::visible::{self, Handles};
use super::*;
use crate::akroma_family_public_v40::GameOutcomeFactsV40;
type Error = HumanDecisionErrorV1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanGameOutcomeFamilyV40 {
    pub sources: Vec<super::game_outcome_v41::HumanGameOutcomeSourceV41>,
    pub cannot_lose: [bool; 2],
    pub cannot_win: [bool; 2],
    pub roots: Vec<HumanGameOutcomeRulesV40>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanGameOutcomeRulesV40 {
    pub name: String,
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
    pub static_prohibition_has_no_target_payment_choice_or_saved_duration: bool,
}

pub(super) fn register(facts: &GameOutcomeFactsV40, handles: &mut Handles) -> Result<(), Error> {
    for row in &facts.protection.sources {
        handles.insert(&row.source)?;
    }
    Ok(())
}

pub(super) fn state(
    facts: &GameOutcomeFactsV40,
    handles: &Handles,
) -> Result<HumanGameOutcomeFamilyV40, Error> {
    let mut sources = facts
        .protection
        .sources
        .iter()
        .map(|row| {
            Ok(super::game_outcome_v41::HumanGameOutcomeSourceV41 {
                source: handles.get(&row.source)?.clone(),
                printed_rules_active: row.printed_rules_active,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    sources.sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    let roots = facts
        .roots
        .iter()
        .map(|root| {
            Ok(HumanGameOutcomeRulesV40 {
                name: crate::card_def::CARD_DEFS
                    .get(usize::from(root.card_id))
                    .ok_or(Error::InvalidVisibleReference)?
                    .name
                    .into(),
                printed_cost: root.printed_cost.clone(),
                flash: root.flash,
                flying: root.flying,
                controller_cannot_lose_game: root.controller_cannot_lose_game,
                opponents_cannot_win_game: root.opponents_cannot_win_game,
                current_controller_and_active_battlefield_printed_ability_required: root
                    .current_controller_and_active_battlefield_printed_ability_required,
                applies_to_life_poison_and_empty_draw_losses: root
                    .applies_to_life_poison_and_empty_draw_losses,
                protection_sampled_before_simultaneous_sba_departures: root
                    .protection_sampled_before_simultaneous_sba_departures,
                protected_empty_draw_attempt_expires_at_its_sba_check: root
                    .protected_empty_draw_attempt_expires_at_its_sba_check,
                life_and_poison_conditions_persist_after_source_leaves: root
                    .life_and_poison_conditions_persist_after_source_leaves,
                static_prohibition_has_no_target_payment_choice_or_saved_duration: root
                    .static_prohibition_has_no_target_payment_choice_or_saved_duration,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanGameOutcomeFamilyV40 {
        sources,
        roots,
        cannot_lose: facts.protection.cannot_lose,
        cannot_win: facts.protection.cannot_win,
    })
}

pub(super) fn unary(
    facts: &HumanGameOutcomeFamilyV40,
    unary: &mut std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &facts.sources {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    Ok(())
}
