//! Scoped public program companion. V9 observations retain their own identity.
//! No new scorer, tensor, weights, profile or model admission is supplied here.
use crate::policy_observation_v9::ObservationV9;
use crate::rl::RlContractError;
use crate::rules_meaning_ooze_v1::{self, OozeRootMeaningV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/public-ooze/v1";
pub fn source_sha256() -> String {
    let mut digest = Sha256::new();
    digest.update(rules_meaning_ooze_v1::source_sha256().as_bytes());
    for source in [
        include_str!("ooze_public_v1.rs"),
        include_str!("human_bo3_v1/ooze_v11.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("policy_observation_v9.rs"),
        include_str!("rl_session/human_v8.rs"),
        include_str!("rl_session/raid_v9.rs"),
    ] {
        digest.update([0]);
        digest.update(source.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OozePublicObservationV1 {
    pub schema: String,
    pub source_sha256: String,
    pub predecessor: ObservationV9,
    pub atomic_activation_roots: Vec<OozeRootMeaningV1>,
    pub projection_sha256: String,
}
fn digest(record: &OozePublicObservationV1) -> Result<String, RlContractError> {
    let bytes = serde_json::to_vec(&(
        &record.schema,
        &record.source_sha256,
        &record.predecessor,
        &record.atomic_activation_roots,
    ))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
pub(crate) fn from_authenticated_v9(
    base: ObservationV9,
) -> Result<OozePublicObservationV1, RlContractError> {
    if base.schema_version != 9 || base.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH {
        return Err(RlContractError("Ooze predecessor identity mismatch".into()));
    }
    let mut record = OozePublicObservationV1 {
        schema: SCHEMA.into(),
        source_sha256: source_sha256(),
        predecessor: base,
        atomic_activation_roots: rules_meaning_ooze_v1::catalog_roots(),
        projection_sha256: String::new(),
    };
    record.projection_sha256 = digest(&record)?;
    Ok(record)
}
/// Artifact identity validation only. Execution additionally revalidates the
/// private complete session binding; a recomputed public digest grants no authority.
pub fn validate(record: &OozePublicObservationV1) -> Result<(), RlContractError> {
    if record.schema != SCHEMA
        || record.source_sha256 != source_sha256()
        || record.predecessor.schema_version != 9
        || record.predecessor.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
        || record.atomic_activation_roots != rules_meaning_ooze_v1::catalog_roots()
        || record.projection_sha256 != digest(record)?
    {
        return Err(RlContractError("Ooze public identity mismatch".into()));
    }
    Ok(())
}
