//! Validated public replacement facts for the pending V33 consumer.
//! This transport does not admit a predecessor observation or model.
use crate::event::{CommittedEvent, ReplacementEffectKind};
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::state::{GameState, StackTargetContractV4, Target, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/fiery-public-facts/v1";

/// Transport authority only. The owning Human projector must replace this
/// relation with an actor-visible row. No concealed card definition is emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIncarnationV1 {
    pub arena_id: u32,
    pub zone_change_count: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleV1 {
    Unexpired,
    ExpiredAtCleanup,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathReplacementV1 {
    pub source_at_resolution: CardStableRefV1,
    pub original_target: TargetIncarnationV1,
    pub original_target_present: bool,
    pub ending_turn_key: u32,
    pub ending_active_player: PlayerSeatV1,
    pub lifecycle: LifecycleV1,
    pub printed_root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
    pub death_replacements: Vec<DeathReplacementV1>,
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("fiery_public_v1.rs"),
        include_str!("fiery_annihilation_v1.rs"),
        include_str!("rules_meaning_fiery_v1.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("engine.rs"),
        include_str!("effect.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Fresh projection from validated original installations and real expiry.
/// Unexpired does not imply the target is still present or still a creature.
pub fn facts_v1(state: &GameState) -> Result<FactsV1, RlContractError> {
    crate::event::validate_exile_if_dies_replacements(state)
        .map_err(|_| RlContractError("invalid Fiery replacement lifecycle".into()))?;
    let mut death_replacements = Vec::new();
    for event in &state.engine.event_history {
        let CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement } = event else {
            continue;
        };
        let ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 {
            target,
            source,
            turn,
            active_player,
        } = replacement.kind
        else {
            return Err(RlContractError(
                "invalid Fiery installed replacement kind".into(),
            ));
        };
        let StackTargetContractV4::Object {
            object,
            zone: Zone::Battlefield,
            zone_change_count,
            ..
        } = target
        else {
            return Err(RlContractError("invalid Fiery original target".into()));
        };
        let selector = crate::rules_meaning_fiery_v1::live_selector().map_err(|_| {
            RlContractError("Fiery public facts require selected Limited catalog".into())
        })?;
        let meaning = crate::rules_meaning_fiery_v1::meaning(&selector, source.card_def, 0)
            .map_err(|_| {
                RlContractError("Fiery public facts require genuine printed source".into())
            })?;
        let expired = state.engine.event_history.iter().any(|event| {
            matches!(event,
            CommittedEvent::ExileIfDiesReplacementExpiredV1 { replacement: original, .. }
            if original == replacement)
        });
        death_replacements.push(DeathReplacementV1 {
            source_at_resolution: CardStableRefV1 {
                arena_id: source.source.0,
                card_db_id: source.card_def,
                owner: source.owner.into(),
                controller: source.controller.into(),
                zone: source.zone,
                zone_change_count: source.zone_change_count,
            },
            original_target: TargetIncarnationV1 {
                arena_id: object.0,
                zone_change_count,
            },
            original_target_present: crate::fiery_annihilation_v1::original_battlefield_target(
                state,
                Target::Object(object),
                target,
            ),
            ending_turn_key: turn,
            ending_active_player: active_player.into(),
            lifecycle: if expired {
                LifecycleV1::ExpiredAtCleanup
            } else {
                LifecycleV1::Unexpired
            },
            printed_root_sha256: meaning.root_sha256,
        });
    }
    Ok(FactsV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
        death_replacements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_absence_is_readonly_and_contains_no_private_target_definition() {
        let state = crate::policy_observation_v6::tests::ready_state();
        let before = serde_json::to_vec(&state).unwrap();
        let facts = facts_v1(&state).unwrap();
        assert!(facts.death_replacements.is_empty());
        assert_eq!(facts_v1(&state).unwrap(), facts);
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let target = serde_json::to_value(TargetIncarnationV1 {
            arena_id: 7,
            zone_change_count: 3,
        })
        .unwrap();
        assert_eq!(target.as_object().unwrap().len(), 2);
        assert!(target.get("card_db_id").is_none());
    }
}
