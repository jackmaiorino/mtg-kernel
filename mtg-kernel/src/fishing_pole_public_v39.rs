//! Authenticated Fishing facts. Historical hosts expose incarnation only.
use crate::event::CommittedEvent;
use crate::fishing_pole_v39::lifecycle::JournalV39;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/fishing-pole-public/v39";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV39 {
    pub arena_id: u32,
    pub zone_change_count: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV39 {
    Uncollected,
    Pending,
    OnStack,
    Resolved,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoleV39 {
    pub source: CardStableRefV1,
    pub bait: i32,
    pub attached_host: Option<IncarnationV39>,
    pub current_named_pole_cost_candidate: bool,
    pub printed_abilities_active: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationV39 {
    pub original_pole: CardStableRefV1,
    pub original_host: IncarnationV39,
    pub original_controller: PlayerSeatV1,
    pub trigger: bool,
    pub stage: StageV39,
    pub stack_ordinal: Option<u64>,
    pub bait_before: Option<i32>,
    pub bait_after: Option<i32>,
    pub created_fish: Option<IncarnationV39>,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV39 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
    pub poles: Vec<PoleV39>,
    pub operations: Vec<OperationV39>,
}
pub fn producer_sha256_v39() -> String {
    let mut hash = Sha256::new();
    for text in [
        SCHEMA,
        include_str!("fishing_pole_public_v39.rs"),
        include_str!("fishing_pole_v39.rs"),
        include_str!("fishing_pole_v39/lifecycle.rs"),
        include_str!("rules_meaning_fishing_pole_v39.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("effect.rs"),
        include_str!("card_def.rs"),
        include_str!("trigger.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("rl.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(text.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn stable(s: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: s.source.0,
        card_db_id: s.card_def,
        owner: s.owner.into(),
        controller: s.controller.into(),
        zone: s.zone,
        zone_change_count: s.zone_change_count,
    }
}
fn root(id: u16, trigger: bool) -> Result<String, RlContractError> {
    let selector = crate::rules_meaning_fishing_pole_v39::live_selector()
        .map_err(|_| RlContractError("Fishing requires pure Limited selected roots".into()))?;
    crate::rules_meaning_fishing_pole_v39::meaning(
        &selector,
        id,
        if trigger {
            crate::rules_meaning_fishing_pole_v39::RootV39::EquippedCreatureUntaps
        } else {
            crate::rules_meaning_fishing_pole_v39::RootV39::GrantedBaitActivation
        },
    )
    .map(|meaning| meaning.root_sha256)
    .map_err(|_| RlContractError("Fishing printed source root changed".into()))
}
pub fn facts_v39(state: &GameState) -> Result<FactsV39, RlContractError> {
    crate::fishing_pole_v39::lifecycle::validate_state(state).map_err(RlContractError)?;
    let mut poles = Vec::new();
    for (id, live) in state.objects.iter() {
        if live.zone != Zone::Battlefield
            || !crate::fishing_pole_v39::selected_definition(live.card_def)
            || live.v4.face_down_v1.is_some()
        {
            continue;
        }
        let host = live.v4.attached_to;
        poles.push(PoleV39 {
            source: crate::rl::card_ref(state, id)?,
            bait: live.counters.bait,
            attached_host: host.map(|link| IncarnationV39 {
                arena_id: link.object.0,
                zone_change_count: link.zone_change_count,
            }),
            current_named_pole_cost_candidate: host.is_some_and(|link| {
                crate::fishing_pole_v39::cost_candidates(
                    state,
                    state.objects.get(link.object).controller,
                    link.object,
                )
                .contains(&id)
            }),
            printed_abilities_active:
                crate::continuous_characteristics_v1::printed_abilities_active(state, id),
        });
    }
    let mut operations = Vec::new();
    for (origin, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::FishingPoleJournalV39 { journal } = event else {
            continue;
        };
        let (pole, host, trigger) = match journal.as_ref() {
            JournalV39::PaymentInput { pole, host, .. } => (*pole, *host, false),
            JournalV39::Captured { pole, untap } => {
                let Some(CommittedEvent::FishingPoleJournalV39 { journal }) =
                    state.engine.event_history.get(*untap as usize)
                else {
                    return Err(RlContractError("Fishing original untap missing".into()));
                };
                let JournalV39::Untapped { host, .. } = journal.as_ref() else {
                    return Err(RlContractError("Fishing original untap changed".into()));
                };
                (*pole, *host, true)
            }
            _ => continue,
        };
        let mut published = None;
        let mut collected = false;
        let mut removed = None;
        let mut after = None;
        let mut token = None;
        for e in &state.engine.event_history {
            if let CommittedEvent::FishingPoleJournalV39 { journal } = e {
                match journal.as_ref() {
                    JournalV39::Published {
                        origin: saved,
                        item,
                    } if *saved == origin as u64 => published = Some(item.as_ref()),
                    JournalV39::Collected { capture } if *capture == origin as u64 => {
                        collected = true
                    }
                    JournalV39::Removed {
                        origin: saved,
                        countered,
                        bait,
                        ..
                    } if *saved == origin as u64 => removed = Some((*countered, *bait)),
                    JournalV39::BaitChanged {
                        origin: saved,
                        after: count,
                        ..
                    } if *saved == origin as u64 => after = Some(*count),
                    JournalV39::Completed {
                        origin: saved,
                        token: created,
                        ..
                    } if *saved == origin as u64 => token = *created,
                    _ => {}
                }
            }
        }
        let created_fish = token
            .map(|at| match state.engine.event_history.get(at as usize) {
                Some(CommittedEvent::CreateToken { object, .. }) => Ok(IncarnationV39 {
                    arena_id: object.0,
                    zone_change_count: 0,
                }),
                _ => Err(RlContractError("Fishing Fish occurrence absent".into())),
            })
            .transpose()?;
        operations.push(OperationV39 {
            original_pole: stable(pole),
            original_host: IncarnationV39 {
                arena_id: host.source.0,
                zone_change_count: host.zone_change_count,
            },
            original_controller: pole.controller.into(),
            trigger,
            stage: match removed {
                Some((true, _)) => StageV39::Countered,
                Some((false, _)) => StageV39::Resolved,
                None if published.is_some() => StageV39::OnStack,
                None if collected => StageV39::Pending,
                None => StageV39::Uncollected,
            },
            stack_ordinal: published
                .and_then(|item| {
                    state
                        .stack
                        .iter()
                        .position(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                })
                .map(|index| u64::try_from(index).expect("stack ordinal fits u64")),
            bait_before: removed.and_then(|(_, before)| before),
            bait_after: after,
            created_fish,
            root_sha256: root(pole.card_def, trigger)?,
        });
    }
    Ok(FactsV39 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v39(),
        poles,
        operations,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_actual_absence_has_no_fishing_identity_or_mutation_both_seats() {
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let state = crate::policy_observation_v6::tests::ready_state_for(actor);
            let before = serde_json::to_vec(&state).unwrap();
            let facts = facts_v39(&state).unwrap();
            assert!(facts.poles.is_empty() && facts.operations.is_empty());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            assert_eq!(
                serde_json::to_value(IncarnationV39 {
                    arena_id: 17,
                    zone_change_count: 3
                })
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
                2
            );
        }
    }
}
