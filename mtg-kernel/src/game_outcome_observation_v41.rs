//! Herald supplement over the coherent V40 public transport.
//! Action authority requires the owning V41 binding, never this serialized record.
use crate::akroma_public_v1::AkromaObservationV1;
use crate::game_outcome_protection_public_v1::{self as protection, PublicFactsV1};
use crate::ids::PlayerId;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::RlContractError;
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/game-outcome-observation/v41";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameOutcomeObservationV41 {
    pub schema: String,
    pub producer_sha256: String,
    pub predecessor: AkromaObservationV1,
    pub protection: PublicFactsV1,
    pub projection_sha256: String,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn producer_sha256_v41() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("game_outcome_observation_v41.rs"),
        include_str!("game_outcome_protection_public_v1.rs"),
        include_str!("rules_meaning_game_outcome_protection_v1.rs"),
        include_str!("game_outcome_protection_v1.rs"),
        include_str!("rl_session/game_outcome_v41.rs"),
        include_str!("human_bo3_v1/game_outcome_v41.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.update(crate::akroma_public_v1::producer_sha256_v1().as_bytes());
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn commitment(record: &GameOutcomeObservationV41) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(record).map_err(|e| RlContractError(e.to_string()))?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Herald observation shape".into()))?
        .remove("projection_sha256");
    Ok(digest(
        &serde_json::to_vec(&value).map_err(|e| RlContractError(e.to_string()))?,
    ))
}

impl GameOutcomeObservationV41 {
    /// Artifact identity is not authentication of current engine facts.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        self.predecessor.validate_artifact()?;
        if self.schema != SCHEMA
            || self.producer_sha256 != producer_sha256_v41()
            || self.protection.schema != protection::SCHEMA
            || self.protection.selector
                != crate::rules_meaning_game_outcome_protection_v1::live_selector()
                    .map_err(|_| RlContractError("Herald selector".into()))?
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError(
                "Herald observation identity mismatch".into(),
            ));
        }
        Ok(())
    }
}

pub fn observe_v41(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<GameOutcomeObservationV41, RlContractError> {
    let predecessor =
        crate::akroma_public_v1::observe_v1(state, surface, actor, step, physical, substep)?;
    let selector = crate::rules_meaning_game_outcome_protection_v1::live_selector()
        .map_err(|_| RlContractError("Herald selector unavailable".into()))?;
    let protection = protection::capture(state, &selector)
        .map_err(|_| RlContractError("Herald public facts disagree with engine".into()))?;
    let common = predecessor.families.iter().find_map(|family| match family {
        crate::akroma_family_public_v40::FamilyFactsV40::GameOutcome(facts) => {
            Some(&facts.protection)
        }
        _ => None,
    });
    if common != Some(&protection) {
        return Err(RlContractError(
            "Herald supplement differs from common family".into(),
        ));
    }
    let mut record = GameOutcomeObservationV41 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256_v41(),
        predecessor,
        protection,
        projection_sha256: String::new(),
    };
    record.projection_sha256 = commitment(&record)?;
    Ok(record)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    #[test]
    fn absent_family_keeps_actor_privacy_and_rejects_forged_status_both_seats() {
        use crate::policy_observation_v6::tests::{put, ready_state_for};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            put(&mut state, actor, "Forest", crate::state::Zone::Hand);
            put(
                &mut state,
                actor.opponent(),
                "Lightning Bolt",
                crate::state::Zone::Hand,
            );
            let before = serde_json::to_vec(&state).unwrap();
            let surface = PolicySurfaceV5::new_for_session();
            let old =
                crate::akroma_public_v1::observe_v1(&state, &surface, actor, 0, 0, (0, 1)).unwrap();
            let record = observe_v41(&state, &surface, actor, 0, 0, (0, 1)).unwrap();
            assert_eq!(record.predecessor, old);
            assert!(record.protection.sources.is_empty());
            assert_eq!(record.protection.cannot_lose, [false; 2]);
            record.validate_artifact().unwrap();
            let mut forged = record;
            forged.protection.cannot_lose[actor.index()] = true;
            assert!(forged.validate_artifact().is_err());
            // Even a recomputed commitment cannot authenticate claimed current status.
            forged.projection_sha256 = commitment(&forged).unwrap();
            assert!(protection::validate(&state, &forged.protection).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}
