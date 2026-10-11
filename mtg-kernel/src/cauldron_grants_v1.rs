//! Exact provenance for an activated ability borrowed from an exiled card.
use crate::state::AbilitySourceContractV4;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CauldronGrantV1 {
    pub host: AbilitySourceContractV4,
    pub donor: AbilitySourceContractV4,
    pub local_index: u16,
}

/// An absent grant preserves the hashes of preexisting snapshots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CauldronGrantRecordV1(pub Option<CauldronGrantV1>);

impl CauldronGrantRecordV1 {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}

impl Hash for CauldronGrantRecordV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if let Some(grant) = self.0 {
            b"cauldron_grant_v1".hash(state);
            grant.hash(state);
        }
    }
}
