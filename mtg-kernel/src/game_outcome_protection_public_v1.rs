//! Current public battlefield facts for the printed game-outcome prohibition.
//! This supplement needs an owning successor consumer before action admission.
use crate::card_def::{GameOutcomeProtectionDefV1, CARD_DEFS};
use crate::ids::PlayerId;
use crate::rl::{CardStableRefV1, PlayerSeatV1};
use crate::rules_meaning_game_outcome_protection_v1::{self as rules, MeaningV1, SelectorV1};
use crate::state::{GameState, Zone};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "mtg-kernel/game-outcome-protection-public/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceV1 {
    pub source: CardStableRefV1,
    pub printed_meaning: MeaningV1,
    pub printed_rules_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicFactsV1 {
    pub schema: String,
    pub selector: SelectorV1,
    pub sources: Vec<SourceV1>,
    pub cannot_lose: [bool; 2],
    pub cannot_win: [bool; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorV1 {
    Meaning(rules::ErrorV1),
    InvalidPublicSource,
    EngineDisagreement,
    IdentityMismatch,
}

impl From<rules::ErrorV1> for ErrorV1 {
    fn from(error: rules::ErrorV1) -> Self {
        Self::Meaning(error)
    }
}

/// Does not enumerate hand, library, stack, departed or face-down identities.
/// Rows remain visible when a face-up Herald has its printed ability suppressed.
pub fn capture(state: &GameState, selector: &SelectorV1) -> Result<PublicFactsV1, ErrorV1> {
    if *selector != rules::live_selector()? {
        return Err(ErrorV1::Meaning(rules::ErrorV1::WrongSelector));
    }
    let mut sources = Vec::new();
    let mut cannot_lose = [false; 2];
    for (id, object) in state.objects.iter() {
        // Visibility is checked before looking up the underlying definition.
        if object.zone != Zone::Battlefield
            || object.v4.face_down_v1.is_some()
            || object.v4.face_index != 0
        {
            continue;
        }
        let Some(def) = CARD_DEFS.get(usize::from(object.card_def)) else {
            return Err(ErrorV1::InvalidPublicSource);
        };
        if def.game_outcome_protection_v1
            != Some(GameOutcomeProtectionDefV1::ControllerCannotLoseAndOpponentsCannotWin)
        {
            continue;
        }
        if object.owner.0 > 1 || object.controller.0 > 1 {
            return Err(ErrorV1::InvalidPublicSource);
        }
        let printed_meaning = rules::meaning(selector, object.card_def)?;
        let printed_rules_active = crate::game_outcome_protection_v1::active(state, id);
        cannot_lose[object.controller.index()] |= printed_rules_active;
        sources.push(SourceV1 {
            source: CardStableRefV1 {
                arena_id: id.0,
                card_db_id: object.card_def,
                owner: PlayerSeatV1::from(object.owner),
                controller: PlayerSeatV1::from(object.controller),
                zone: Zone::Battlefield,
                zone_change_count: object.zone_change_count,
            },
            printed_meaning,
            printed_rules_active,
        });
    }
    let cannot_win = [cannot_lose[1], cannot_lose[0]];
    for player in [PlayerId::P0, PlayerId::P1] {
        if cannot_lose[player.index()]
            != crate::game_outcome_protection_v1::cannot_lose(state, player)
            || cannot_win[player.index()]
                != crate::game_outcome_protection_v1::cannot_win(state, player)
        {
            return Err(ErrorV1::EngineDisagreement);
        }
    }
    Ok(PublicFactsV1 {
        schema: SCHEMA.into(),
        selector: selector.clone(),
        sources,
        cannot_lose,
        cannot_win,
    })
}

/// Complete current-state rederivation, not validation of self-declared facts.
pub fn validate(state: &GameState, record: &PublicFactsV1) -> Result<(), ErrorV1> {
    if *record != capture(state, &record.selector)? {
        return Err(ErrorV1::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_printed_family_projects_no_sources_without_mutating_state() {
        let Ok(selector) = rules::live_selector() else {
            return;
        };
        for player in [PlayerId::P0, PlayerId::P1] {
            let state = crate::policy_observation_v6::tests::ready_state_for(player);
            let bytes = serde_json::to_vec(&state).unwrap();
            let hash = state.state_hash();
            let record = capture(&state, &selector).unwrap();
            assert!(record.sources.is_empty());
            assert_eq!(record.cannot_lose, [false; 2]);
            assert_eq!(record.cannot_win, [false; 2]);
            validate(&state, &record).unwrap();
            let mut forged = record.clone();
            forged.cannot_lose[player.index()] = true;
            assert_eq!(validate(&state, &forged), Err(ErrorV1::IdentityMismatch));
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
            assert_eq!(state.state_hash(), hash);
        }
    }
}
