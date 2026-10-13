//! Ajani-only facts for an explicit V42 mixed producer. No hidden historical
//! target printing is exported, even if a former target has reentered publicly.
use crate::ajani_full_v42::lifecycle::JournalV42;
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::rules_meaning_ajani_v42::{MeaningV42, RootV42};
use crate::state::{GameState, StackTargetContractV4, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/ajani-public/v42";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV42 {
    pub arena_id: u32,
    pub zone_change_count: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV42 {
    OnStack,
    Resolved,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV42 {
    pub source: CardStableRefV1,
    pub loyalty: u32,
    pub used_this_turn: bool,
    pub printed_abilities_active: bool,
    pub controller_life: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV42 {
    pub occurrence: u64,
    pub source_at_payment: CardStableRefV1,
    pub current_original_source: Option<CardStableRefV1>,
    pub ability_index: u8,
    pub loyalty_before: u32,
    pub loyalty_after: u32,
    pub stage: StageV42,
    pub target: Option<IncarnationV42>,
    pub current_original_target: Option<CardStableRefV1>,
    pub target_legal_at_resolution: Option<bool>,
    pub controller_life_at_resolution: Option<i32>,
    pub created_tokens: Vec<IncarnationV42>,
    pub keyword_grant_expired: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV42 {
    pub permanents: Vec<PermanentV42>,
    pub activations: Vec<ActivationV42>,
    pub meanings: Vec<MeaningV42>,
}
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for s in [
        SCHEMA,
        include_str!("ajani_public_v42.rs"),
        include_str!("ajani_full_v42.rs"),
        include_str!("ajani_full_v42/lifecycle.rs"),
        include_str!("rules_meaning_ajani_v42.rs"),
        include_str!("planeswalker_v1.rs"),
        include_str!("rl.rs"),
        include_str!("state.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("controlled_creature_grants_v1.rs"),
        include_str!("card_def.rs"),
    ] {
        h.update([0]);
        h.update(s.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn current(
    state: &GameState,
    i: IncarnationV42,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let id = ObjectId(i.arena_id);
    if state
        .objects
        .try_get(id)
        .is_some_and(|o| o.zone == Zone::Battlefield && o.zone_change_count == i.zone_change_count)
    {
        crate::rl::card_ref(state, id).map(Some)
    } else {
        Ok(None)
    }
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::ajani_full_v42::lifecycle::validate_state(state)
}
pub fn facts(state: &GameState) -> Result<FactsV42, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let mut permanents = Vec::new();
    for (id, o) in state.objects.iter() {
        if o.zone != Zone::Battlefield || !crate::ajani_full_v42::selected_definition(o.card_def) {
            continue;
        }
        crate::ajani_full_v42::validate_definition(o.card_def).map_err(RlContractError)?;
        permanents.push(PermanentV42 {
            source: crate::rl::card_ref(state, id)?,
            loyalty: crate::planeswalker_v1::loyalty(state, id)
                .ok_or_else(|| RlContractError("Ajani public loyalty ledger missing".into()))?,
            used_this_turn: crate::ajani_full_v42::lifecycle::used(state, id),
            printed_abilities_active:
                crate::continuous_characteristics_v1::printed_abilities_active(state, id),
            controller_life: state.players[o.controller.index()].life,
        });
    }
    let history = &state.engine.event_history;
    let mut activations = Vec::new();
    for e in history {
        let CommittedEvent::AjaniJournalV42 { journal } = e else {
            continue;
        };
        let JournalV42::Paid { index, input, item } = journal.as_ref() else {
            continue;
        };
        let CommittedEvent::AjaniJournalV42 { journal: original } = &history[*input as usize]
        else {
            unreachable!("validated paid input");
        };
        let JournalV42::PaymentInput {
            source,
            before,
            after,
            ..
        } = original.as_ref()
        else {
            unreachable!("validated input kind");
        };
        let removed = history.iter().find_map(|e| match e {
            CommittedEvent::AjaniJournalV42 { journal } => match journal.as_ref() {
                JournalV42::RemovalInput {
                    index: at,
                    paid,
                    countered,
                    life,
                    target,
                    ..
                } if paid == index => Some((*at, *countered, *life, *target)),
                _ => None,
            },
            _ => None,
        });
        let target = item.v4.target_contracts.first().and_then(|t| match t {
            StackTargetContractV4::Object {
                object,
                zone_change_count,
                ..
            } => Some(IncarnationV42 {
                arena_id: object.0,
                zone_change_count: *zone_change_count,
            }),
            _ => None,
        });
        let mut tokens = Vec::new();
        let mut expired = None;
        if let Some((removed, _, _, _)) = removed {
            for e in history {
                if let CommittedEvent::AjaniJournalV42 { journal } = e {
                    if let JournalV42::Operation {
                        input,
                        events,
                        boost,
                        ..
                    } = journal.as_ref()
                    {
                        if *input == removed {
                            for at in events {
                                if let CommittedEvent::CreateToken { object, .. } =
                                    history[*at as usize]
                                {
                                    tokens.push(IncarnationV42 {
                                        arena_id: object.0,
                                        zone_change_count: 0,
                                    });
                                }
                            }
                            if let Some(b) = boost {
                                expired=Some(history.iter().any(|e|matches!(e,CommittedEvent::AjaniJournalV42{journal} if matches!(journal.as_ref(),JournalV42::Cleanup{boosts,..} if boosts.contains(b)))));
                            }
                        }
                    }
                }
            }
        }
        activations.push(ActivationV42 {
            occurrence: *index,
            source_at_payment: CardStableRefV1 {
                arena_id: source.source.0,
                card_db_id: source.card_def,
                owner: source.owner.into(),
                controller: source.controller.into(),
                zone: source.zone,
                zone_change_count: source.zone_change_count,
            },
            current_original_source: current(
                state,
                IncarnationV42 {
                    arena_id: source.source.0,
                    zone_change_count: source.zone_change_count,
                },
            )?,
            ability_index: item.v4.activated_ability_index.unwrap(),
            loyalty_before: *before,
            loyalty_after: *after,
            stage: match removed {
                None => StageV42::OnStack,
                Some((_, true, _, _)) => StageV42::Countered,
                _ => StageV42::Resolved,
            },
            target,
            current_original_target: target.map(|t| current(state, t)).transpose()?.flatten(),
            target_legal_at_resolution: removed
                .filter(|(_, c, _, _)| !*c && item.v4.activated_ability_index != Some(2))
                .map(|(_, _, _, t)| t.is_some()),
            controller_life_at_resolution: removed.and_then(|(_, _, life, _)| life),
            created_tokens: tokens,
            keyword_grant_expired: expired,
        });
    }
    let meanings = meanings()?;
    Ok(FactsV42 {
        permanents,
        activations,
        meanings,
    })
}

pub(crate) fn meanings() -> Result<Vec<MeaningV42>, RlContractError> {
    let mut meanings = Vec::new();
    for (id, _) in crate::card_def::CARD_DEFS
        .iter()
        .enumerate()
        .filter(|(id, _)| {
            u16::try_from(*id)
                .ok()
                .is_some_and(crate::ajani_full_v42::selected_definition)
        })
    {
        let selector = crate::rules_meaning_ajani_v42::live_selector()
            .map_err(|_| RlContractError("Ajani precise catalog selector unavailable".into()))?;
        for root in [
            RootV42::PrintedPlaneswalkerSpell,
            RootV42::PlusOneCounter,
            RootV42::MinusThreeKeywords,
            RootV42::MinusEightCats,
        ] {
            meanings.push(
                crate::rules_meaning_ajani_v42::meaning(&selector, id as u16, root)
                    .map_err(|_| RlContractError("Ajani precise root unavailable".into()))?,
            );
        }
    }
    Ok(meanings)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_partial_recipient_has_no_fabricated_full_facts() {
        let state = crate::policy_observation_v6::tests::ready_state_for(crate::ids::PlayerId::P0);
        let before = serde_json::to_vec(&state).unwrap();
        let f = facts(&state).unwrap();
        assert!(f.permanents.is_empty() && f.activations.is_empty());
        let expected = if crate::card_def::CARD_DEFS.iter().enumerate().any(|(i, _)| {
            u16::try_from(i)
                .ok()
                .is_some_and(crate::ajani_full_v42::selected_definition)
        }) {
            4
        } else {
            0
        };
        assert_eq!(f.meanings.len(), expected);
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}
