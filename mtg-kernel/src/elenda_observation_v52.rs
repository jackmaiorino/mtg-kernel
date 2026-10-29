//! Complete V51 public envelope plus current Elenda facts, with a distinct owner contract.
use crate::akroma_family_public_v40::diagnose_observation_error as diagnostic;
use crate::{
    ids::PlayerId, policy_surface_v5::PolicySurfaceV5,
    ravenous_observation_v51::RavenousObservationV51, rl::RlContractError, state::GameState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/elenda-observation/v52";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElendaObservationV52 {
    pub schema: String,
    pub producer_sha256: String,
    /// Complete inherited fields, exact wide roster and every existing family.
    /// This internal component does not authorize a predecessor consumer.
    pub common: RavenousObservationV51,
    pub elenda: crate::elenda_public_v52::FactsV52,
    pub projection_sha256: String,
}
pub(crate) type ElendaPublicPartsV52 = (
    ElendaObservationV52,
    Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
);

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        let elenda = crate::elenda_public_v52::producer_sha256();
        let ravenous = crate::ravenous_observation_v51::producer_sha256();
        for part in [
            SCHEMA,
            include_str!("elenda_observation_v52.rs"),
            include_str!("rl_session/elenda_v52.rs"),
            include_str!("rl_session.rs"),
            elenda.as_str(),
            ravenous.as_str(),
        ] {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.update(b"legacy-graveyard-return-guard/v61\0");
        hash.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub fn commitment(value: &ElendaObservationV52) -> Result<String, RlContractError> {
    let mut copy = value.clone();
    copy.projection_sha256.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|e| RlContractError(e.to_string()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::engine::validate_battlefield_pt(state)
        .map_err(|id| format!("native PT envelope exceeded for {id:?}"))?;
    crate::ravenous_observation_v51::validate_state(state)?;
    crate::elenda_public_v52::capture(state).map_err(|e| e.0)?;
    Ok(())
}
pub fn observe_v52(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<ElendaObservationV52, RlContractError> {
    observe_parts_v52(state, surface, actor_and_origin, step, physical, substep)
        .map(|(value, _)| value)
}
pub(crate) fn observe_parts_v52(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<ElendaPublicPartsV52, RlContractError> {
    validate_state(state)
        .map_err(|error| diagnostic("elenda.state", error))
        .map_err(RlContractError)?;
    let (common, families) = crate::ravenous_observation_v51::observe_parts_complete_base_v52(
        state,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
    )
    .map_err(|error| diagnostic("elenda.inherited", error))?;
    common
        .validate_artifact()
        .map_err(|error| diagnostic("elenda.inherited_artifact", error))?;
    let mut value = ElendaObservationV52 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        common,
        elenda: crate::elenda_public_v52::capture(state)
            .map_err(|error| diagnostic("elenda.facts", error))?,
        projection_sha256: String::new(),
    };
    value.projection_sha256 =
        commitment(&value).map_err(|error| diagnostic("elenda.commitment", error))?;
    Ok((value, families))
}
pub fn validate_current(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
    value: &ElendaObservationV52,
) -> Result<(), RlContractError> {
    if observe_v52(state, surface, actor_and_origin, step, physical, substep)? == *value {
        Ok(())
    } else {
        Err(RlContractError(
            "Elenda V52 observation differs from current owned decision".into(),
        ))
    }
}
