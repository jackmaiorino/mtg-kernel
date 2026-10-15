//! Authenticated public counts and outcomes; historical member identities stay private.
use crate::event::CommittedEvent;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::state::{GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/krenko-public-facts/v1";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleV1 {
    PaidOnStack,
    Resolved,
    Countered,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenIncarnationV1 {
    pub arena_id: u32,
    pub zone_change_count: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV1 {
    pub source_at_payment: CardStableRefV1,
    pub captured_controller: PlayerSeatV1,
    pub stack_ordinal: Option<u64>,
    pub lifecycle: LifecycleV1,
    pub sampled_goblins: Option<u64>,
    pub created_tokens: Vec<TokenIncarnationV1>,
    pub printed_root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceV1 {
    pub source: CardStableRefV1,
    pub current_controlled_goblin_permanents: u64,
    pub printed_root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
    pub sources: Vec<SourceV1>,
    pub activations: Vec<ActivationV1>,
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("krenko_public_v1.rs"),
        include_str!("krenko_v1.rs"),
        include_str!("rules_meaning_krenko_v1.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("state.rs"),
        include_str!("card_def.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn root(id: u16) -> Result<String, RlContractError> {
    let selector = crate::rules_meaning_krenko_v1::live_selector()
        .map_err(|_| RlContractError("Krenko requires selected Limited catalog".into()))?;
    crate::rules_meaning_krenko_v1::meaning(
        &selector,
        id,
        crate::rules_meaning_krenko_v1::RootV1::PrintedTapActivation { ability_index: 0 },
    )
    .map(|m| m.root_sha256)
    .map_err(|_| RlContractError("Krenko printed root changed".into()))
}
pub fn facts_v1(state: &GameState) -> Result<FactsV1, RlContractError> {
    crate::krenko_v1::validate_state(state).map_err(RlContractError)?;
    let mut sources = Vec::new();
    for (id, o) in state.objects.iter() {
        if o.zone == Zone::Battlefield
            && crate::krenko_v1::selected_definition(o.card_def)
            && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        {
            sources.push(SourceV1 {
                source: crate::rl::card_ref(state, id)?,
                current_controlled_goblin_permanents: u64::try_from(
                    crate::krenko_v1::members(state, o.controller).len(),
                )
                .map_err(|_| RlContractError("Goblin count exceeds u64".into()))?,
                printed_root_sha256: root(o.card_def)?,
            });
        }
    }
    let mut activations = Vec::new();
    for event in &state.engine.event_history {
        let CommittedEvent::KrenkoPaidV1 { paid } = event else {
            continue;
        };
        let source = paid
            .item
            .v4
            .ability_source_contract
            .ok_or_else(|| RlContractError("Krenko source lost".into()))?;
        let removed = state.engine.event_history.iter().find_map(|e| match e {
            CommittedEvent::KrenkoRemovedV1 { removed } if removed.paid == paid.index => {
                Some(removed.as_ref())
            }
            _ => None,
        });
        let completion = removed.and_then(|r| {
            state.engine.event_history.iter().find_map(|e| match e {
                CommittedEvent::KrenkoCompletedV1 { completed } if completed.removed == r.index => {
                    Some(completed.as_ref())
                }
                _ => None,
            })
        });
        let sample = completion
            .and_then(|c| c.sample)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| state.engine.event_history.get(i))
            .and_then(|e| match e {
                CommittedEvent::KrenkoSampleV1 { sample } => Some(sample.as_ref()),
                _ => None,
            });
        let mut created_tokens = Vec::new();
        if let Some(c) = completion {
            for index in &c.token_history {
                let index = usize::try_from(*index)
                    .map_err(|_| RlContractError("Krenko history exceeds usize".into()))?;
                let Some(CommittedEvent::CreateToken { object, .. }) =
                    state.engine.event_history.get(index)
                else {
                    return Err(RlContractError("Krenko token occurrence lost".into()));
                };
                // CreateToken creates its new object at the original generation0.
                // A later move must never link the historical token to a reentry.
                created_tokens.push(TokenIncarnationV1 {
                    arena_id: object.0,
                    zone_change_count: 0,
                });
            }
        }
        activations.push(ActivationV1 {
            source_at_payment: CardStableRefV1 {
                arena_id: source.source.0,
                card_db_id: source.card_def,
                owner: source.owner.into(),
                controller: source.controller.into(),
                zone: source.zone,
                zone_change_count: source.zone_change_count,
            },
            captured_controller: source.controller.into(),
            stack_ordinal: state
                .stack
                .iter()
                .position(|i| i.v4.stack_item_id == paid.item.v4.stack_item_id)
                .map(|i| u64::try_from(i).expect("stack ordinal fits u64")),
            lifecycle: match removed.map(|r| r.kind) {
                None => LifecycleV1::PaidOnStack,
                Some(crate::krenko_v1::RemovalV1::Resolving) => LifecycleV1::Resolved,
                Some(crate::krenko_v1::RemovalV1::Countered) => LifecycleV1::Countered,
            },
            sampled_goblins: sample
                .map(|s| {
                    u64::try_from(s.members.len())
                        .map_err(|_| RlContractError("Krenko count exceeds u64".into()))
                })
                .transpose()?,
            created_tokens,
            printed_root_sha256: root(source.card_def)?,
        });
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
        sources,
        activations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_absence_has_no_historical_member_identity_or_mutation() {
        let state = crate::policy_observation_v6::tests::ready_state();
        let bytes = serde_json::to_vec(&state).unwrap();
        let facts = facts_v1(&state).unwrap();
        assert!(facts.sources.is_empty() && facts.activations.is_empty());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        let value = serde_json::to_value(TokenIncarnationV1 {
            arena_id: 7,
            zone_change_count: 0,
        })
        .unwrap();
        assert_eq!(value.as_object().unwrap().len(), 2);
        assert!(value.get("card_db_id").is_none());
    }
}
