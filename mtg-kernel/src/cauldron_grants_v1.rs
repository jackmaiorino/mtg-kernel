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

#[cfg(all(test, feature = "standard-magezero-fixtures"))]
mod tests {
    #[test]
    fn the_registered_pool_fits_cauldron_action_indices_for_two_four_copy_decks() {
        use crate::card_def::{CardType, CARD_DEFS};
        let mut all_distinct = 0;
        let mut duplicate_limited = 0;
        for def in CARD_DEFS
            .iter()
            .filter(|def| def.has_type(CardType::Creature))
        {
            // Deliberately count back-face and nonbattlefield abilities too.
            all_distinct += def.activated_abilities.len() + def.mana_ability_choices.len();
            duplicate_limited += 7 * def
                .activated_abilities
                .iter()
                .filter(|a| a.max_activations_per_turn.is_some())
                .count();
            if def.name == "Surge Engine" {
                duplicate_limited += 7;
            }
        }
        let host = CARD_DEFS
            .iter()
            .map(|def| def.activated_abilities.len())
            .max()
            .unwrap_or(0);
        assert!(all_distinct + duplicate_limited + host + 1 < 255,
            "Cauldron needs wider action indices: {all_distinct} distinct + {duplicate_limited} duplicate limited + {host} host + Equipment gap");
    }
}
